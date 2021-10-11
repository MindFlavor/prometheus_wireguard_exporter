use crate::metrics::MetricAttributeOptions;

#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub prepend_sudo: bool,
    pub extract_names_config_files: Option<Vec<String>>,
    pub interfaces: Option<Vec<String>>,
    pub metric_attributes: MetricAttributeOptions,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        let handshake_timeout_seconds = matches
            .value_of("handshake_timeout_seconds")
            .map(|timeout| timeout.parse::<u64>().unwrap());

        let options = Options {
            verbose: matches.is_present("verbose"),
            prepend_sudo: matches.is_present("prepend_sudo"),
            metric_attributes: MetricAttributeOptions {
                split_allowed_ips: matches.is_present("separate_allowed_ips"),
                export_remote_ip_and_port: matches.is_present("export_remote_ip_and_port"),
                handshake_timeout_seconds,
            },
            extract_names_config_files: matches
                .values_of("extract_names_config_files")
                .map(|e| e.into_iter().map(|e| e.to_owned()).collect()),
            interfaces: matches
                .values_of("interfaces")
                .map(|e| e.into_iter().map(|a| a.to_owned()).collect()),
        };

        options
    }
}
