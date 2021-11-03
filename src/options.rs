#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub prepend_sudo: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_files: Option<Vec<String>>,
    pub interfaces: Option<Vec<String>>,
    pub export_remote_ip_and_port: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        let options = Options {
            verbose: matches
                .value_of("verbose")
                .map(|e| {
                    e.to_lowercase()
                        .parse()
                        .expect("cannot parse verbose as a bool")
                })
                .unwrap_or_default(),
            prepend_sudo: matches
                .value_of("prepend_sudo")
                .map(|e| {
                    e.to_lowercase()
                        .parse()
                        .expect("cannot parse prepend_sudo as a bool")
                })
                .unwrap_or_default(),
            separate_allowed_ips: matches
                .value_of("separate_allowed_ips")
                .map(|e| {
                    e.to_lowercase()
                        .parse()
                        .expect("cannot parse separate_allowed_ips as a bool")
                })
                .unwrap_or_default(),
            extract_names_config_files: matches
                .values_of("extract_names_config_files")
                .map(|e| e.into_iter().map(|e| e.to_owned()).collect()),
            interfaces: matches
                .values_of("interfaces")
                .map(|e| e.into_iter().map(|a| a.to_owned()).collect()),
            export_remote_ip_and_port: matches
                .value_of("export_remote_ip_and_port")
                .map(|e| {
                    e.to_lowercase()
                        .parse()
                        .expect("cannot parse export_remote_ip_and_port as a bool")
                })
                .unwrap_or_default(),
        };

        options
    }
}
