use std::{fmt, str::FromStr};

use mime::Mime;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};

/// Wrapper around [Mime] to allow serializing and deserializing it
#[derive(Debug)]
pub struct WrappedMime(pub Mime);

impl Serialize for WrappedMime {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serialize_mime(&self.0, serializer)
    }
}

impl<'de> Deserialize<'de> for WrappedMime {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserialize_mime(deserializer).map(Self)
    }
}

pub fn serialize_mime<S>(mime: &Mime, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_str(mime.as_ref())
}

pub fn deserialize_mime<'de, D>(deserializer: D) -> Result<Mime, D::Error>
where
    D: Deserializer<'de>,
{
    struct Visitor;

    impl de::Visitor<'_> for Visitor {
        type Value = Mime;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("a valid MIME type")
        }

        fn visit_str<E>(self, value: &str) -> Result<Mime, E>
        where
            E: de::Error,
        {
            Mime::from_str(value).map_err(|e| E::custom(format!("{}", e)))
        }
    }

    deserializer.deserialize_str(Visitor)
}
