use crate::context::Context;
use hyper::{
    header::{HeaderName, HeaderValue},
    HeaderMap,
};
use serde::{
    de::{self, MapAccess, Visitor},
    Deserialize, Deserializer, Serialize,
};
use shellexpand::env_with_context_no_errors;
use tracing::debug;

pub type HeadersTransformsList = Vec<HeaderTransform>;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct HeaderTransform {
    action: HeaderTransformActon,
    value: Option<String>,
}

impl HeaderTransform {
    fn action(&self) -> &HeaderTransformActon {
        &self.action
    }

    fn value(&self) -> Option<&String> {
        self.value.as_ref()
    }
}

#[derive(Deserialize, Debug, Clone, PartialEq, Serialize)]
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

pub trait HeadersTransformator<'a> {
    fn transform(&'a self, headers: &'a mut HeaderMap, ctx: &Context);
}

impl<'a> HeadersTransformator<'a> for HeadersTransformsList {
    fn transform(&'a self, headers: &'a mut HeaderMap, ctx: &Context) {
        for transform in self {
            match transform.action().clone() {
                HeaderTransformActon::Add(key) => {
                    if !headers.contains_key(key.clone()) {
                        let value = transform.value().as_ref().unwrap().as_str();
                        let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                        let key = HeaderName::from_bytes(key.as_bytes()).unwrap();
                        debug!("add: name={key}, value={value}");
                        headers.insert(&key, HeaderValue::from_str(&value).unwrap());
                    }
                }
                HeaderTransformActon::Update(key) => {
                    if headers.contains_key(&key) {
                        let value = transform.value().as_ref().unwrap().as_str();
                        let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                        let debug_key = key.clone();
                        let key = HeaderName::from_bytes(key.as_bytes()).unwrap();
                        let old = headers.insert(key, HeaderValue::from_str(&value).unwrap());
                        if let Some(old) = old {
                            debug!(
                                "update: name={}, old={}, new={}",
                                debug_key,
                                old.to_str().unwrap(),
                                value
                            );
                        } else {
                            debug!("update: name={}, old=, new={}", debug_key, value);
                        }
                    }
                }
                HeaderTransformActon::Drop(key) => {
                    if key == "*" {
                        debug!("drop: all headers");
                        headers.clear();
                    } else {
                        let debug_key = key.clone();
                        let old = headers.remove(key);
                        if let Some(old) = old {
                            debug!("drop: name={}, old={}", debug_key, old.to_str().unwrap());
                        } else {
                            debug!("drop: name={}, old=", debug_key);
                        }
                    }
                }
            };
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::{assert_debug_snapshot, assert_ron_snapshot};

    use crate::context::test_context::get_test_ctx;

    use super::*;

    #[test]
    fn deserialize_ok_header_transform() {
        let add_action: HeaderTransform =
            serde_json::from_str(r#"{"add": "new_header", "value": "new_value"}"#).unwrap();
        assert_ron_snapshot!(add_action);

        let update_action: HeaderTransform =
            serde_json::from_str(r#"{"update": "new_header", "value": "new_value"}"#).unwrap();
        assert_ron_snapshot!(update_action);

        let drop_action: HeaderTransform =
            serde_json::from_str(r#"{"drop": "old_header"}"#).unwrap();
        assert_ron_snapshot!(drop_action);
    }

    #[test]
    fn deserialize_wrong_header_transform() {
        let wrong_json = [
            r#"{"wrong_action": "new_header", "value": "new_value"}"#,
            r#"{"wrong_action": "new_header"}"#,
            r#"{"add": "new_header", "wrong_value": "new_value"}"#,
            r#"{"add": "new_header", "value": 1}"#,
            r#"{"add": "new_header"}"#,
            r#"{"update": "new_header", "wrong_value": "new_value"}"#,
            r#"{"update": "new_header", "value": 1}"#,
            r#"{"update": "new_header"}"#,
            r#"{"drop": "old_header", "wrong_value": "old_value"}"#,
            r#"{"drop": "old_header", "value": "old_value"}"#,
            r#"{"drop": "old_header", "value": 1}"#,
        ];

        for test_item in wrong_json {
            let result: Result<HeaderTransform, serde_json::Error> =
                serde_json::from_str(test_item);
            assert!(
                result.is_err(),
                "unexpected deserialization of `{}`",
                test_item
            );
        }
    }

    #[test]
    fn transforms() {
        let ctx = get_test_ctx();
        assert_ron_snapshot!(ctx, {".own" => insta::sorted_redaction(), ".parent.own" => insta::sorted_redaction()});

        let transforms: HeadersTransformsList = serde_json::from_str(
            r#"
        [
            {"add": "X-Some-New-Header", "value": "good"},
            {"update": "X-Existing-Header", "value": "good"},
            {"drop": "X-Header-To-Drop"},
            {"add": "X-Existing-Header", "value": "wrong"},
            {"update": "X-Non-Exiting-Header", "value": "wrong"},
            {"add": "X-Env-Header", "value": "${TEST_ENV_HEADER_TO_ADD}"},
            {"update": "X-Env-Header-2", "value": "${TEST_ENV_HEADER_TO_ADD}"}
        ]
        "#,
        )
        .unwrap();
        assert_debug_snapshot!(transforms);

        let mut headers = HeaderMap::new();
        headers.insert("X-Header-To-Drop", "wrong".parse().unwrap());
        headers.insert("X-Existing-Header", "wrong".parse().unwrap());
        headers.insert("X-Header-Without-Changes", "good".parse().unwrap());
        headers.insert("X-Env-Header-2", "wrong".parse().unwrap());
        // Before transformation
        assert_debug_snapshot!(headers);

        transforms.transform(&mut headers, ctx);
        // After transformation
        assert_debug_snapshot!(headers);
    }
}
