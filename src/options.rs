#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_file: Option<String>,
    pub export_remote_ip_and_port: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        if let Some(e) = matches.value_of("extract_names_config_file") {
            Options {
                verbose: matches.is_present("verbose"),
                separate_allowed_ips: matches.is_present("separate_allowed_ips"),
                extract_names_config_file: Some(e.to_owned()),
                export_remote_ip_and_port: matches.is_present("export_remote_ip_and_port"),
            }
        } else {
            Options {
                verbose: matches.is_present("verbose"),
                separate_allowed_ips: matches.is_present("separate_allowed_ips"),
                extract_names_config_file: None,
                export_remote_ip_and_port: matches.is_present("export_remote_ip_and_port"),
            }
        }
    }

    pub fn get_interface(&self) -> Option<&str> {
        if let Some(config_file) = &self.extract_names_config_file {
            let path = std::path::Path::new(config_file);
            if let Some(file_stem) = path.file_stem() {
                file_stem.to_str()
            } else {
                None
            }
        } else {
            None
        }
    }
}
