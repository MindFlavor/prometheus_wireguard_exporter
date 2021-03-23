use crate::exporter_error::FriendlyDescritionParseError;
use std::borrow::Cow;
use std::collections::HashMap;
use std::convert::TryFrom;

#[derive(Debug, Clone, PartialEq)]
pub enum FriendlyDescription<'a> {
    Name(Cow<'a, str>),
    Json(HashMap<&'a str, serde_json::Value>),
}

impl<'a> TryFrom<(&'a str, &'a str)> for FriendlyDescription<'a> {
    type Error = FriendlyDescritionParseError;

    fn try_from((header_name, value): (&'a str, &'a str)) -> Result<Self, Self::Error> {
        Ok(match header_name {
            "friendly_name" => FriendlyDescription::Name(value.into()),
            "friendly_json" => {
                let ret: HashMap<&str, serde_json::Value> = serde_json::from_str(value)?;
                FriendlyDescription::Json(ret)
            }

            other => {
                return Err(FriendlyDescritionParseError::UnsupportedHeader(format!(
                    "{} is not a supported tag",
                    other
                )))
            }
        })
    }
}
