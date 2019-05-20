extern crate serde_json;
#[macro_use]
extern crate failure;
use clap;
use clap::Arg;
use futures::future::{done, ok, Either, Future};
use http::StatusCode;
use hyper::service::service_fn;
use hyper::{Body, Request, Response, Server};
use log::{error, info, trace};
use std::env;
mod options;
use options::Options;
mod exporter_error;
use exporter_error::ExporterError;
mod render_to_prometheus;
use render_to_prometheus::RenderToPrometheus;
mod wireguard;
use std::convert::TryFrom;
use std::process::Command;
use std::string::String;
use wireguard::WireGuard;
mod wireguard_config;
use wireguard_config::PeerEntryHashMap;

fn check_compliance(req: &Request<Body>) -> Result<(), Response<Body>> {
    if req.uri() != "/metrics" {
        trace!("uri not allowed {}", req.uri());
        Err(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(hyper::Body::empty())
            .unwrap())
    } else if req.method() != "GET" {
        trace!("method not allowed {}", req.method());
        Err(Response::builder()
            .status(StatusCode::METHOD_NOT_ALLOWED)
            .body(hyper::Body::empty())
            .unwrap())
    } else {
        Ok(())
    }
}

fn handle_request(
    req: Request<Body>,
    options: Options,
) -> impl Future<Item = Response<Body>, Error = failure::Error> {
    trace!("{:?}", req);

    done(check_compliance(&req)).then(move |res| match res {
        Ok(_) => Either::A(perform_request(req, &options).then(|res| match res {
            Ok(body) => ok(body),
            Err(err) => {
                error!("internal server error: {:?}", err);
                ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(hyper::Body::empty())
                    .unwrap())
            }
        })),
        Err(err) => Either::B(ok(err)),
    })
}

fn wg_with_text(
    wg_config_str: &str,
    wg_output: ::std::process::Output,
) -> Result<Response<Body>, ExporterError> {
    let pehm = PeerEntryHashMap::try_from(wg_config_str)?;
    println!("pehm == {:?}", pehm);

    let wg_output_string = String::from_utf8(wg_output.stdout)?;
    let wg = WireGuard::try_from(&wg_output_string as &str)?;
    Ok(Response::new(Body::from(wg.render())))
}

fn perform_request(
    _req: Request<Body>,
    options: &Options,
) -> impl Future<Item = Response<Body>, Error = ExporterError> {
    trace!("perform_request");

    // this is needed to satisfy the borrow checker
    let options = options.clone();

    done(
        Command::new("wg")
            .arg("show")
            .arg("all")
            .arg("dump")
            .output(),
    )
    .from_err()
    .and_then(|output| {
        if let Some(extract_names_config_file) = options.extract_names_config_file {
            Either::A(
                done(::std::fs::read_to_string(extract_names_config_file))
                    .from_err()
                    .and_then(|wg_config_string| wg_with_text(&wg_config_string as &str, output)),
            )
        } else {
            Either::B(
                done(String::from_utf8(output.stdout))
                    .from_err()
                    .and_then(|output_str| {
                        trace!("{}", output_str);
                        done(WireGuard::try_from(&output_str as &str))
                            .from_err()
                            .and_then(|wg| ok(Response::new(Body::from(wg.render()))))
                    }),
            )
        }
    })
}

fn main() {
    let matches = clap::App::new("prometheus_wireguard_exporter")
        .version("0.1")
        .author("Francesco Cogno <francesco.cogno@outlook.com>")
        .arg(
            Arg::with_name("port")
                .short("p")
                .help("exporter port")
                .default_value("9576")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .help("verbose logging")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("extract_names_config_file")
                .short("n")
                .help("If set, the exporter will look in the specified WireGuard config file for peer names (must be in [Peer] definition and be a comment)")
                .takes_value(true))
        .get_matches();

    let options = Options::from_claps(&matches);

    if options.verbose {
        env::set_var("RUST_LOG", "prometheus_wireguard_exporter=trace");
    } else {
        env::set_var("RUST_LOG", "prometheus_wireguard_exporter=info");
    }
    env_logger::init();

    info!("using options: {:?}", options);

    let bind = matches.value_of("port").unwrap();
    let bind = u16::from_str_radix(&bind, 10).expect("port must be a valid number");
    let addr = ([0, 0, 0, 0], bind).into();

    info!("starting exporter on {}", addr);

    let new_svc = move || {
        let options = options.clone();
        service_fn(move |req| handle_request(req, options.clone()))
    };

    let server = Server::bind(&addr)
        .serve(new_svc)
        .map_err(|e| eprintln!("server error: {}", e));
    hyper::rt::run(server);
}
