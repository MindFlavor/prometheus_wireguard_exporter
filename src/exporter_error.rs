#[derive(Debug, Fail)]
pub enum PeerEntryParseError {
    #[fail(display = "PublicKey entry not found in lines: {:?}", lines)]
    PublicKeyNotFound { lines: Vec<String> },

    #[fail(display = "AllowedIPs entry not found in lines: {:?}", lines)]
    AllowedIPsEntryNotFound { lines: Vec<String> },
}

#[derive(Debug, Fail)]
pub enum ExporterError {
    #[allow(dead_code)]
    #[fail(display = "Generic error")]
    Generic {},

    #[fail(display = "Hyper error: {}", e)]
    Hyper { e: hyper::error::Error },

    #[fail(display = "http error: {}", e)]
    Http { e: http::Error },

    #[fail(display = "UTF-8 error: {}", e)]
    UTF8 { e: std::string::FromUtf8Error },

    #[fail(display = "JSON format error: {}", e)]
    JSON { e: serde_json::error::Error },

    #[fail(display = "IO Error: {}", e)]
    IO { e: std::io::Error },

    #[fail(display = "UTF8 conversion error: {}", e)]
    Utf8 { e: std::str::Utf8Error },

    #[fail(display = "int conversion error: {}", e)]
    ParseInt { e: std::num::ParseIntError },

    #[fail(display = "PeerEntry parse error: {}", e)]
    PeerEntryParseError { e: PeerEntryParseError },
}

impl From<PeerEntryParseError> for ExporterError {
    fn from(e: PeerEntryParseError) -> Self {
        ExporterError::PeerEntryParseError { e }
    }
}

impl From<std::io::Error> for ExporterError {
    fn from(e: std::io::Error) -> Self {
        ExporterError::IO { e }
    }
}

impl From<hyper::error::Error> for ExporterError {
    fn from(e: hyper::error::Error) -> Self {
        ExporterError::Hyper { e }
    }
}

impl From<http::Error> for ExporterError {
    fn from(e: http::Error) -> Self {
        ExporterError::Http { e }
    }
}

impl From<std::string::FromUtf8Error> for ExporterError {
    fn from(e: std::string::FromUtf8Error) -> Self {
        ExporterError::UTF8 { e }
    }
}

impl From<serde_json::error::Error> for ExporterError {
    fn from(e: serde_json::error::Error) -> Self {
        ExporterError::JSON { e }
    }
}

impl From<std::str::Utf8Error> for ExporterError {
    fn from(e: std::str::Utf8Error) -> Self {
        ExporterError::Utf8 { e }
    }
}

impl From<std::num::ParseIntError> for ExporterError {
    fn from(e: std::num::ParseIntError) -> Self {
        ExporterError::ParseInt { e }
    }
}
