use hyper::header::HeaderValue;
use hyper::HeaderMap;
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};
use shellexpand::env_with_context_no_errors;

use crate::context::Context;

pub type HeadersTransformsList = Vec<HeaderTransform>;

#[derive(Debug, Clone)]
pub struct HeaderTransform {
    action: HeaderTransformActon,
    value: Option<String>,
}

impl HeaderTransform {
    pub fn action(&self) -> &HeaderTransformActon {
        &self.action
    }

    pub fn value(&self) -> Option<&String> {
        self.value.as_ref()
    }
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum HeaderTransformActon {
    Add(String),
    Update(String),
    Drop(String),
}

impl<'de> Deserialize<'de> for HeaderTransform {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize, Debug)]
        #[serde(deny_unknown_fields, rename_all = "lowercase")]
        enum Fields {
            Drop,
            Add,
            Update,
            Value,
        }

        struct HeaderTransformVisitor;
        impl<'de> Visitor<'de> for HeaderTransformVisitor {
            type Value = HeaderTransform;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str("struct HeaderTransform")
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: MapAccess<'de>,
            {
                let mut action: Option<HeaderTransformActon> = None;
                let mut value: Option<String> = None;

                // Extract all fields
                while let Some(key) = map.next_key()? {
                    match key {
                        Fields::Add => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("add"));
                            }
                            action = Some(HeaderTransformActon::Add(map.next_value::<String>()?))
                        }
                        Fields::Drop => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("drop"));
                            }
                            action = Some(HeaderTransformActon::Drop(map.next_value::<String>()?))
                        }
                        Fields::Update => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("update"));
                            }
                            action = Some(HeaderTransformActon::Update(map.next_value::<String>()?))
                        }
                        Fields::Value => {
                            if value.is_some() {
                                return Err(de::Error::duplicate_field("value"));
                            }
                            value = Some(map.next_value::<String>()?)
                        }
                    }
                }

                if let Some(action) = action {
                    match action {
                        HeaderTransformActon::Add(_) | HeaderTransformActon::Update(_) => {
                            if value.is_none() {
                                return Err(de::Error::missing_field("value"));
                            }
                        }
                        HeaderTransformActon::Drop(_) => {
                            if value.is_some() {
                                return Err(de::Error::custom(
                                    "unknown field `value` in action drop",
                                ));
                            }
                        }
                    }
                    Ok(HeaderTransform { action, value })
                } else {
                    Err(de::Error::missing_field(
                        "action should be one of add/drop/update",
                    ))
                }
            }
        }

        const FIELDS: &[&str] = &["add", "drop", "update", "value"];
        deserializer.deserialize_struct("HeaderAction", FIELDS, HeaderTransformVisitor)
    }
}

pub trait HeadersTransformator {
    fn transform(&'static self, headers: &mut HeaderMap, ctx: &Context);
}

impl HeadersTransformator for HeadersTransformsList {
    fn transform(&'static self, headers: &mut HeaderMap, ctx: &Context) {
        for transform in self {
            match transform.action() {
                HeaderTransformActon::Add(key) => {
                    if !headers.contains_key(key.clone()) {
                        let value = transform.value().as_ref().unwrap().as_str();
                        let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                        headers.insert(key.as_str(), HeaderValue::from_str(&value).unwrap());
                    }
                }
                HeaderTransformActon::Update(key) => {
                    if headers.contains_key(key) {
                        let value = transform.value().as_ref().unwrap().as_str();
                        let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                        headers.insert(key.as_str(), HeaderValue::from_str(&value).unwrap());
                    }
                }
                HeaderTransformActon::Drop(key) => {
                    if key == "*" {
                        headers.clear();
                    } else {
                        headers.remove(key);
                    }
                }
            };
        }
    }
}
