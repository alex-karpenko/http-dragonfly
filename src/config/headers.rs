use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer,
};

#[derive(Debug, Clone)]
pub struct HeaderTransform {
    pub action: HeaderTransformActon,
    pub value: Option<String>,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields, rename_all = "lowercase")]
pub enum HeaderTransformActon {
    Add(String),
    Replace(String),
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
            Replace,
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
                        Fields::Replace => {
                            if action.is_some() {
                                return Err(de::Error::duplicate_field("replace"));
                            }
                            action =
                                Some(HeaderTransformActon::Replace(map.next_value::<String>()?))
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
                        HeaderTransformActon::Add(_) | HeaderTransformActon::Replace(_) => {
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
                        "action should be one of add/drop/replace",
                    ))
                }
            }
        }

        const FIELDS: &[&str] = &["add", "drop", "replace", "value"];
        deserializer.deserialize_struct("HeaderAction", FIELDS, HeaderTransformVisitor)
    }
}