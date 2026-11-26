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
                    } else if key.contains('*') {
                        let to_drop: Vec<HeaderName> = headers
                            .keys()
                            .filter(|name| wildcard_match(&key, name.as_str()))
                            .cloned()
                            .collect();
                        if to_drop.is_empty() {
                            debug!("drop: pattern={key}, nothing matched");
                        }
                        for name in to_drop {
                            headers.remove(&name);
                            debug!("drop: pattern={key}, name={name}");
                        }
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

/// Case-insensitive glob match of a header name against a pattern,
/// where `*` matches any (possibly empty) sequence of characters.
fn wildcard_match(pattern: &str, name: &str) -> bool {
    let pattern = pattern.as_bytes();
    let name = name.as_bytes();

    let (mut p, mut n) = (0, 0);
    // Position of the last seen `*` in pattern and the name position it was tried at.
    let mut backtrack: Option<(usize, usize)> = None;

    while n < name.len() {
        if p < pattern.len() && pattern[p] == b'*' {
            backtrack = Some((p, n));
            p += 1;
        } else if p < pattern.len() && pattern[p].eq_ignore_ascii_case(&name[n]) {
            p += 1;
            n += 1;
        } else if let Some((star_p, star_n)) = backtrack {
            // Let the last `*` consume one more character and retry.
            p = star_p + 1;
            n = star_n + 1;
            backtrack = Some((star_p, n));
        } else {
            return false;
        }
    }

    pattern[p..].iter().all(|&c| c == b'*')
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
                "unexpected deserialization of `{test_item}`"
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

    #[test]
    fn wildcard_matching() {
        let cases = [
            ("x-amzn-*", "x-amzn-trace-id", true),
            ("x-amzn-*", "x-amzn-", true),
            ("X-Amzn-*", "x-amzn-requestid", true),
            ("x-amzn-*", "x-amz-date", false),
            ("x-amzn-*", "prefix-x-amzn-id", false),
            ("*-id", "x-request-id", true),
            ("*-id", "x-request-ids", false),
            ("x-*-id", "x-request-id", true),
            ("x-*-id", "x-id", false),
            ("x-*-*-id", "x-a-b-id", true),
            ("x-*-*-id", "x-a-id", false),
            ("*amzn*", "x-amzn-trace-id", true),
            ("*amzn*", "amzn", true),
            ("*amzn*", "x-amz-date", false),
            ("**", "anything", true),
            ("*", "", true),
            ("a*b*c", "aXbYbZc", true),
            ("a*b*c", "aXbYcZ", false),
        ];

        for (pattern, name, expected) in cases {
            assert_eq!(
                wildcard_match(pattern, name),
                expected,
                "pattern `{pattern}` vs name `{name}`"
            );
        }
    }

    #[test]
    fn wildcard_drop_transforms() {
        let ctx = get_test_ctx();

        let transforms: HeadersTransformsList = serde_json::from_str(
            r#"
        [
            {"drop": "X-Amzn-*"},
            {"drop": "*-trace"},
            {"drop": "x-no-match-*"}
        ]
        "#,
        )
        .unwrap();

        let mut headers = HeaderMap::new();
        headers.insert("x-amzn-trace-id", "drop".parse().unwrap());
        headers.insert("x-amzn-requestid", "drop".parse().unwrap());
        headers.append("x-amzn-multi", "drop-1".parse().unwrap());
        headers.append("x-amzn-multi", "drop-2".parse().unwrap());
        headers.insert("x-b3-trace", "drop".parse().unwrap());
        headers.insert("x-amz-date", "keep".parse().unwrap());
        headers.insert("x-trace-keep", "keep".parse().unwrap());
        headers.insert("content-type", "keep".parse().unwrap());

        transforms.transform(&mut headers, ctx);

        let mut remaining: Vec<&str> = headers.keys().map(|k| k.as_str()).collect();
        remaining.sort_unstable();
        assert_eq!(remaining, ["content-type", "x-amz-date", "x-trace-keep"]);
        assert!(headers.values().all(|v| v == "keep"));
    }
}
