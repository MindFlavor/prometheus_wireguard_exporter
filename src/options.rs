#[derive(Debug, Clone)]
pub(crate) struct Options {
    pub verbose: bool,
}

impl Options {
    pub fn from_claps(matches: &clap::ArgMatches<'_>) -> Options {
        Options {
            verbose: matches.is_present("verbose"),
        }
    }
}
