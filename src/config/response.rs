use hyper::{Body, Response};
use regex::Regex;
use serde::Deserialize;
use shellexpand::env_with_context_no_errors;

use crate::{context::Context, handler::ResponsesMap};

use super::{
    headers::{HeaderTransform, HeadersTransformator},
    ConfigValidator,
};

pub type ResponseStatus = u16;

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields, default)]
pub struct ResponseConfig {
    target_selector: Option<String>,
    failed_status_regex: String,
    no_targets_status: ResponseStatus,
    #[serde(rename = "override")]
    override_config: Option<OverrideConfig>,
}

impl ResponseConfig {
    pub fn target_selector(&self) -> &Option<String> {
        &self.target_selector
    }

    pub fn failed_status_regex(&self) -> &str {
        self.failed_status_regex.as_ref()
    }

    pub fn no_targets_status(&self) -> u16 {
        self.no_targets_status
    }

    pub fn override_config(&self) -> &Option<OverrideConfig> {
        &self.override_config
    }
}

impl Default for ResponseConfig {
    fn default() -> Self {
        Self {
            target_selector: Default::default(),
            failed_status_regex: "4\\d{2}|5\\d{2}".into(),
            no_targets_status: 500,
            override_config: None,
        }
    }
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct OverrideConfig {
    status: Option<ResponseStatus>,
    body: Option<String>,
    headers: Option<Vec<HeaderTransform>>,
}

impl OverrideConfig {
    pub fn status(&self) -> Option<u16> {
        self.status
    }

    pub fn body(&self) -> Option<&String> {
        self.body.as_ref()
    }

    pub fn headers(&self) -> &Option<Vec<HeaderTransform>> {
        &self.headers
    }
}

impl ConfigValidator for ResponseConfig {
    fn validate(&self) -> Result<(), crate::errors::HttpDragonflyError> {
        Ok(())
    }
}

pub trait ResponseBehavior {
    fn override_response(&'static self, resp: Response<Body>, ctx: &Context) -> Response<Body>;
    fn find_first_response(
        &self,
        responses: &ResponsesMap,
        look_for_failed: bool,
    ) -> Option<String>;
}

impl ResponseBehavior for ResponseConfig {
    fn override_response(&'static self, resp: Response<Body>, ctx: &Context) -> Response<Body> {
        if let Some(cfg) = self.override_config() {
            let (resp_parts, resp_body) = resp.into_parts();
            let mut new_resp = Response::builder();

            // Set status
            new_resp = if let Some(status) = cfg.status() {
                new_resp.status(status)
            } else {
                new_resp.status(resp_parts.status)
            };

            // Prepare headers
            let mut headers = resp_parts.headers;
            if let Some(transforms) = &cfg.headers() {
                transforms.transform(&mut headers, ctx)
            }
            for (k, v) in &headers {
                new_resp = new_resp.header(k, v);
            }

            // Prepare body
            let cfg_body = cfg.body();
            let body: Body = if let Some(body) = cfg_body {
                let body: String = env_with_context_no_errors(&body, |v| ctx.get(&v.into())).into();
                Body::from(body)
            } else {
                resp_body
            };

            // Final response
            new_resp.body(body).unwrap()
        } else {
            resp
        }
    }

    fn find_first_response(
        &self,
        responses: &ResponsesMap,
        look_for_failed: bool,
    ) -> Option<String> {
        let re = Regex::new(self.failed_status_regex()).unwrap();
        for key in responses.keys() {
            let (resp, _) = responses.get(key).unwrap();
            if let Some(resp) = resp {
                let status: String = resp.status().to_string();
                if re.is_match(&status) == look_for_failed {
                    // Return first non-failed response target_id
                    return Some(key.into());
                }
            }
        }

        None
    }
}
