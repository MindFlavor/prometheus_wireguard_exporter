#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
    pub separate_allowed_ips: bool,
    pub extract_names_config_files: Option<Vec<String>>,
    pub interfaces: Option<Vec<String>>,
    pub export_remote_ip_and_port: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        let options = Options {
            verbose: matches.is_present("verbose"),
            separate_allowed_ips: matches.is_present("separate_allowed_ips"),
            extract_names_config_files: matches.values_of("extract_names_config_files").map(|e| {
                e.into_iter()
                    .map(|a| {
                        println!("a ==> {}", a);
                        a.to_owned()
                    })
                    .collect()
            }),
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

        if let Some(extract_names_config_files) = &options.extract_names_config_files {
            if let Some(interfaces) = &options.interfaces {
                if extract_names_config_files.len() != interfaces.len() {
                    panic!("syntax error: the number of config_files ({}) and interfaces ({}) options must be equal (or do not specify an interface at all)!", extract_names_config_files.len(),
                        interfaces.len());
                }
            }
        }

        options
    }
}

#[cfg(test)]
mod tests {
    use super::*;
}
