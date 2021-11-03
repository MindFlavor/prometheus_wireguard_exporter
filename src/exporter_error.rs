use thiserror::Error;

#[derive(Error, Debug)]
pub enum FriendlyDescritionParseError {
    #[error("unsupported header")]
    UnsupportedHeader(String),

    #[error("json parse error")]
    SerdeJsonError(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum PeerEntryParseError {
    #[error("PublicKey entry not found in lines: {:?}", lines)]
    PublicKeyNotFound { lines: Vec<String> },

    #[error("AllowedIPs entry not found in lines: {:?}", lines)]
    AllowedIPsEntryNotFound { lines: Vec<String> },

    #[error("Friendly description parse error")]
    FriendlyDescritionParseError(#[from] FriendlyDescritionParseError),
}

#[derive(Debug, Error)]
pub enum ExporterError {
    #[allow(dead_code)]
    #[error("Generic error")]
    Generic {},

    #[error("Hyper error: {}", e)]
    Hyper { e: hyper::Error },

    #[error("http error: {}", e)]
    Http { e: http::Error },

    #[error("UTF-8 error: {}", e)]
    UTF8 { e: std::string::FromUtf8Error },

    #[error("JSON format error: {}", e)]
    Json { e: serde_json::error::Error },

    #[error("IO Error: {}", e)]
    IO { e: std::io::Error },

    #[error("UTF8 conversion error: {}", e)]
    Utf8 { e: std::str::Utf8Error },

    #[error("int conversion error: {}", e)]
    ParseInt { e: std::num::ParseIntError },

    #[error("PeerEntry parse error: {}", e)]
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

impl From<hyper::Error> for ExporterError {
    fn from(e: hyper::Error) -> Self {
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
        ExporterError::Json { e }
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
