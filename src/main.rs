extern crate serde_json;
#[macro_use]
extern crate failure;
use clap;
use clap::{crate_authors, crate_name, crate_version, Arg};
use futures::future::{done, ok, Either, Future};
use hyper::{Body, Request, Response};
use log::{debug, info, trace};
use std::env;
mod options;
use options::Options;
mod wireguard;
use std::convert::TryFrom;
use std::process::Command;
use std::string::String;
use wireguard::WireGuard;
mod exporter_error;
mod wireguard_config;
use wireguard_config::peer_entry_hashmap_try_from;
extern crate prometheus_exporter_base;
use crate::exporter_error::ExporterError;
use prometheus_exporter_base::render_prometheus;
use std::net::Ipv4Addr;
use std::sync::Arc;

fn wg_with_text(
    wg_config_str: &str,
    wg_output_str: &str,
    options: Arc<Options>,
) -> Result<Response<Body>, ExporterError> {
    let pehm = peer_entry_hashmap_try_from(wg_config_str)?;
    trace!("pehm == {:?}", pehm);

    let wg = WireGuard::try_from(wg_output_str)?;
    Ok(Response::new(Body::from(wg.render_with_names(
        Some(&pehm),
        options.separate_allowed_ips,
        options.export_remote_ip_and_port,
    ))))
}

fn perform_request(
    _req: Request<Body>,
    options: &Arc<Options>,
) -> impl Future<Item = Response<Body>, Error = failure::Error> {
    trace!("perform_request");
    // this is needed to satisfy the borrow checker
    let options = options.clone();
    debug!("options == {:?}", options);

    //let interface = options.get_interface();

    let interface_str = match options.get_interface() {
        Some(interface_str) => interface_str,
        None => "all",
    }
    .to_owned();

    debug!("using inteface_str {}", interface_str);

    done(
        Command::new("wg")
            .arg("show")
            .arg(&interface_str)
            .arg("dump")
            .output(),
    )
    .from_err()
    .and_then(move |output| {
        done(String::from_utf8(output.stdout))
            .from_err()
            .and_then(move |output_str| {
                trace!("wg show output == {}", output_str);

                // the output of wg show is different if we use all or we specify an interface.
                // In the first case the first column will be the interface name. In the second case
                // the interface name will be omitted. We need to compensate for the skew somehow (one
                // column less in the second case). We solve this prepending the interface name in every
                // line so the output of the second case will be equal to the first case.
                let output_str = if interface_str != "all" {
                    debug!("injecting {} to the wg show output", interface_str);
                    let mut result = String::new();
                    for s in output_str.lines() {
                        result.push_str(&format!("{}\t{}\n", interface_str, s));
                    }
                    result
                } else {
                    output_str
                };

                if let Some(extract_names_config_file) = &options.extract_names_config_file {
                    Either::A(
                        done(::std::fs::read_to_string(extract_names_config_file))
                            .from_err()
                            .and_then(move |wg_config_string| {
                                wg_with_text(&wg_config_string as &str, &output_str, options)
                            }),
                    )
                } else {
                    Either::B({
                        trace!("{}", output_str);
                        done(WireGuard::try_from(&output_str as &str))
                            .from_err()
                            .and_then(move |wg| {
                                ok(Response::new(Body::from(wg.render_with_names(
                                    None,
                                    options.separate_allowed_ips,
                                    options.export_remote_ip_and_port,
                                ))))
                            })
                    })
                }
            })
            .from_err()
    })
}

fn main() {
    let matches = clap::App::new(crate_name!())
        .version(crate_version!())
        .author(crate_authors!("\n"))
        .arg(
            Arg::with_name("addr")
                .short("l")
                .help("exporter address")
                .default_value("0.0.0.0")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("port")
                .short("p")
                .help("exporter port")
                .default_value("9586")
                .takes_value(true),
        )
        .arg(
            Arg::with_name("verbose")
                .short("v")
                .help("verbose logging")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("separate_allowed_ips")
                .short("s")
                .help("separate allowed ips and ports")
                .takes_value(false),
        )
        .arg(
            Arg::with_name("export_remote_ip_and_port")
                .short("r")
                .help("exports peer's remote ip and port as labels (if available)")
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
        env::set_var(
            "RUST_LOG",
            format!("{}=trace,prometheus_exporter_base=trace", crate_name!()),
        );
    } else {
        env::set_var(
            "RUST_LOG",
            format!("{}=info,prometheus_exporter_base=info", crate_name!()),
        );
    }
    env_logger::init();

    info!("using options: {:?}", options);

    let bind = matches.value_of("port").unwrap();
    let bind = u16::from_str_radix(&bind, 10).expect("port must be a valid number");
    let ip = matches
        .value_of("addr")
        .unwrap()
        .parse::<Ipv4Addr>()
        .unwrap();
    let addr = (ip, bind).into();

    info!("starting exporter on http://{}/metrics", addr);

    render_prometheus(&addr, options, |request, options| {
        Box::new(perform_request(request, options))
    });
}
