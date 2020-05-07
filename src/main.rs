extern crate serde_json;
#[macro_use]
extern crate failure;
use clap::{crate_authors, crate_name, crate_version, Arg};
use hyper::{Body, Request};
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
use prometheus_exporter_base::render_prometheus;
use std::net::IpAddr;
use std::sync::Arc;

fn wg_with_text(
    wg_config_str: &str,
    wg_output_stdout_str: &str,
    options: Arc<Options>,
) -> Result<String, failure::Error> {
    let pehm = peer_entry_hashmap_try_from(wg_config_str)?;
    trace!("pehm == {:?}", pehm);

    let wg = WireGuard::try_from(wg_output_stdout_str)?;
    Ok(wg.render_with_names(
        Some(&pehm),
        options.separate_allowed_ips,
        options.export_remote_ip_and_port,
    ))
}

async fn perform_request(
    _req: Request<Body>,
    options: Arc<Options>,
) -> Result<String, failure::Error> {
    let interfaces_to_handle = match &options.interfaces {
        Some(interfaces_str) => interfaces_str.clone(),
        None => vec!["all".to_owned()],
    };

    let mut result = String::new();

    for (pos, interface_to_handle) in interfaces_to_handle.iter().enumerate() {
        result.push_str(
            &perform_single_request(
                &_req,
                interface_to_handle,
                &options.extract_names_config_file,
                options.clone(),
            )
            .await?,
        );
    }

    Ok(result)
}

async fn perform_single_request(
    _req: &Request<Body>,
    interface_str: &str,
    extract_names_config_file: &Option<String>,
    options: Arc<Options>,
) -> Result<String, failure::Error> {
    trace!("perform_request");
    debug!("inteface_str == {}", interface_str);

    let output = Command::new("wg")
        .arg("show")
        .arg(&interface_str)
        .arg("dump")
        .output()?;
    let output_stdout_str = String::from_utf8(output.stdout)?;
    trace!(
        "wg show {} dump stdout == {}",
        interface_str,
        output_stdout_str
    );
    let output_stderr_str = String::from_utf8(output.stderr)?;
    trace!(
        "wg show {} dump stderr == {}",
        interface_str,
        output_stderr_str
    );

    // the output of wg show is different if we use all or we specify an interface.
    // In the first case the first column will be the interface name. In the second case
    // the interface name will be omitted. We need to compensate for the skew somehow (one
    // column less in the second case). We solve this prepending the interface name in every
    // line so the output of the second case will be equal to the first case.
    let output_stdout_str = if interface_str != "all" {
        debug!("injecting {} to the wg show output", interface_str);
        let mut result = String::new();
        for s in output_stdout_str.lines() {
            result.push_str(&format!("{}\t{}\n", interface_str, s));
        }
        result
    } else {
        output_stdout_str
    };

    if let Some(extract_names_config_file) = extract_names_config_file {
        let wg_config_string = ::std::fs::read_to_string(&extract_names_config_file)?;
        wg_with_text(&wg_config_string as &str, &output_stdout_str, options)
    } else {
        let wg = WireGuard::try_from(&output_stdout_str as &str)?;
        Ok(wg.render_with_names(
            None,
            options.separate_allowed_ips,
            options.export_remote_ip_and_port,
        ))
    }
}

#[tokio::main]
async fn main() {
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
            Arg::with_name("extract_names_config_files")
                .short("n")
                .help("If set, the exporter will look in the specified WireGuard config file for peer names (must be in [Peer] definition and be a comment)")
                .multiple(false)
                .number_of_values(1)
                .takes_value(true))
        .arg(
            Arg::with_name("interfaces")
                .short("i")
                .help("If set specifies the interface passed to the wg show command. It is relative to the same position config_file. In not specified, all will be passed.")
                .multiple(true)
                .number_of_values(1)
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
    let ip = matches.value_of("addr").unwrap().parse::<IpAddr>().unwrap();
    let addr = (ip, bind).into();

    info!("starting exporter on http://{}/metrics", addr);

    render_prometheus(addr, options, |request, options| {
        Box::pin(perform_request(request, options))
    })
    .await;
}
