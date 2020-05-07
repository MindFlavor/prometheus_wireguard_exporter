#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_file: Option<String>,
    pub interfaces: Option<Vec<String>>,
    pub export_remote_ip_and_port: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        let options = Options {
            verbose: matches.is_present("verbose"),
            separate_allowed_ips: matches.is_present("separate_allowed_ips"),
            extract_names_config_file: matches
                .value_of("extract_names_config_files")
                .map(|e| e.to_owned()),
            interfaces: matches.values_of("interfaces").map(|e| {
                e.into_iter()
                    .map(|a| {
                        println!("a ==> {}", a);
                        a.to_owned()
                    })
                    .collect()
            }),
            export_remote_ip_and_port: matches.is_present("export_remote_ip_and_port"),
        };

        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
