#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_file: Option<String>,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        if let Some(e) = matches.value_of("extract_names_config_file") {
            Options {
                verbose: matches.is_present("verbose"),
                separate_allowed_ips: matches.is_present("separate_allowed_ips"),
                extract_names_config_file: Some(e.to_owned()),
            }
        } else {
            Options {
                verbose: matches.is_present("verbose"),
                separate_allowed_ips: matches.is_present("separate_allowed_ips"),
                extract_names_config_file: None,
            }
        }
    }
}
