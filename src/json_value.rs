use serde::{Deserialize, Deserializer, Serialize, Serializer, de, ser::SerializeMap};
use serde_json::value::RawValue;
use std::fmt;

// Raw scalar tokens keep exact numbers without reserving any object keys.
#[derive(Serialize)]
#[serde(untagged)]
pub(crate) enum JsonValue {
    Object(Object<JsonValue>),
    Array(Vec<JsonValue>),
    Scalar(Box<RawValue>),
}

impl JsonValue {
    fn parse(raw: Box<RawValue>, depth: usize) -> serde_json::Result<Self> {
        if depth >= 128 {
            return Err(de::Error::custom("recursion limit exceeded"));
        }
        match raw.get().as_bytes().first() {
            Some(b'{') => {
                let object: Object<Box<RawValue>> = serde_json::from_str(raw.get())?;
                let entries = object
                    .0
                    .into_iter()
                    .map(|(key, value)| Ok((key, Self::parse(value, depth + 1)?)))
                    .collect::<serde_json::Result<_>>()?;
                Ok(Self::Object(Object(entries)))
            }
            Some(b'[') => {
                let array: Vec<Box<RawValue>> = serde_json::from_str(raw.get())?;
                let values = array
                    .into_iter()
                    .map(|value| Self::parse(value, depth + 1))
                    .collect::<serde_json::Result<_>>()?;
                Ok(Self::Array(values))
            }
            _ => Ok(Self::Scalar(raw)),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<&Self> {
        match self {
            Self::Object(object) => object
                .0
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn get_mut(&mut self, key: &str) -> Option<&mut Self> {
        match self {
            Self::Object(object) => object
                .0
                .iter_mut()
                .find(|(name, _)| name == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn as_string(&self) -> Option<String> {
        match self {
            Self::Scalar(raw) => serde_json::from_str(raw.get()).ok(),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for JsonValue {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(deserializer)?;
        Self::parse(raw, 0).map_err(de::Error::custom)
    }
}

pub(crate) struct Object<T>(Vec<(String, T)>);

impl<T: Serialize> Serialize for Object<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Object<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Visitor<T>(std::marker::PhantomData<T>);

        impl<'de, T: Deserialize<'de>> de::Visitor<'de> for Visitor<T> {
            type Value = Object<T>;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<M: de::MapAccess<'de>>(self, mut map: M) -> Result<Object<T>, M::Error> {
                let mut entries: Vec<(String, T)> = Vec::new();
                while let Some((key, value)) = map.next_entry::<String, T>()? {
                    if let Some((_, previous)) = entries.iter_mut().find(|(name, _)| name == &key) {
                        *previous = value;
                    } else {
                        entries.push((key, value));
                    }
                }
                Ok(Object(entries))
            }
        }

        deserializer.deserialize_map(Visitor(std::marker::PhantomData))
    }
}
