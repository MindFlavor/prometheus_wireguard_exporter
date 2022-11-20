use clap::parser::ValuesRef;

#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub prepend_sudo: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_files: Option<Vec<String>>,
    pub interfaces: Option<Vec<String>>,
    pub export_remote_ip_and_port: bool,
    pub export_latest_handshake_delay: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches) -> Options {
        let options = Options {
            verbose: *matches.get_one("verbose").unwrap_or(&false),
            prepend_sudo: *matches.get_one("prepend_sudo").unwrap_or(&false),
            separate_allowed_ips: *matches.get_one("separate_allowed_ips").unwrap_or(&false),
            extract_names_config_files: matches
                .get_many("extract_names_config_files")
                .map(|e: ValuesRef<'_, String>| e.into_iter().map(|a| a.to_owned()).collect()),
            interfaces: matches
                .get_many("interfaces")
                .map(|e: ValuesRef<'_, String>| e.into_iter().map(|a| a.to_string()).collect()),
            export_remote_ip_and_port: *matches
                .get_one("export_remote_ip_and_port")
                .unwrap_or(&false),
            export_latest_handshake_delay: *matches
                .get_one("export_latest_handshake_delay")
                .unwrap_or(&false),
        };

        options
    }
}
