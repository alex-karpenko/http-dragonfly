use hyper::{http::Error, Body, Error as HyperError, Response, StatusCode};
use regex::Regex;
use serde::Deserialize;
use shellexpand::env_with_context_no_errors;
use tracing::debug;

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

impl ConfigValidator for ResponseConfig {
    fn validate(&self) -> Result<(), crate::errors::HttpDragonflyError> {
        Ok(())
    }
}

pub trait ResponseBehavior {
    fn target_selector(&self) -> &Option<String>;
    fn override_response(&'static self, resp: Response<Body>, ctx: &Context) -> Response<Body>;
    fn find_first_response(
        &self,
        responses: &ResponsesMap,
        look_for_failed: bool,
    ) -> Option<String>;
    fn error_response(&self, e: HyperError, status: &Option<ResponseStatus>) -> Response<Body>;
    fn empty_response(&self, status: ResponseStatus) -> Result<Response<Body>, Error>;
    fn override_empty_response(
        &'static self,
        status: ResponseStatus,
        ctx: &Context,
    ) -> Result<Response<Body>, Error>;
    fn no_target_response(&'static self, ctx: &Context) -> Result<Response<Body>, Error>;
    fn select_from_two_targets_response(
        &'static self,
        first_target_id: Option<String>,
        second_target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body>;
    fn select_target_or_override_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body>;
    fn select_target_or_error_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body>;
}

impl ResponseBehavior for ResponseConfig {
    fn target_selector(&self) -> &Option<String> {
        &self.target_selector
    }

    fn override_response(&'static self, resp: Response<Body>, ctx: &Context) -> Response<Body> {
        if let Some(cfg) = &self.override_config {
            let (resp_parts, resp_body) = resp.into_parts();
            let mut new_resp = Response::builder();

            // Set status
            new_resp = if let Some(status) = cfg.status {
                new_resp.status(status)
            } else {
                new_resp.status(resp_parts.status)
            };

            // Prepare headers
            let mut headers = resp_parts.headers;
            if let Some(transforms) = &cfg.headers {
                transforms.transform(&mut headers, ctx)
            }
            for (k, v) in &headers {
                new_resp = new_resp.header(k, v);
            }

            // Prepare body
            let cfg_body = &cfg.body;
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
        debug!(
            "looking for {}",
            if look_for_failed { "failed" } else { "ok" }
        );

        let re = Regex::new(&self.failed_status_regex).unwrap();
        for key in responses.keys() {
            let (resp, _) = responses.get(key).unwrap();
            if let Some(resp) = resp {
                let status: String = resp.status().to_string();
                if re.is_match(&status) == look_for_failed {
                    // Return first non-failed response target_id
                    debug!("found target id={}", key);
                    return Some(key.into());
                }
            }
        }

        debug!("not found any target");
        None
    }

    fn error_response(&self, e: HyperError, status: &Option<ResponseStatus>) -> Response<Body> {
        let resp = Response::builder();
        let resp = if let Some(status) = status.to_owned() {
            resp.status(status)
        } else if e.is_connect() || e.is_closed() {
            resp.status(StatusCode::BAD_GATEWAY)
        } else if e.is_timeout() {
            resp.status(StatusCode::GATEWAY_TIMEOUT)
        } else {
            resp.status(StatusCode::INTERNAL_SERVER_ERROR)
        };

        resp.body(Body::empty()).unwrap()
    }

    fn empty_response(&self, status: ResponseStatus) -> Result<Response<Body>, Error> {
        Response::builder().status(status).body(Body::empty())
    }

    fn override_empty_response(
        &'static self,
        status: ResponseStatus,
        ctx: &Context,
    ) -> Result<Response<Body>, Error> {
        let empty = self.empty_response(status)?;
        Ok(self.override_response(empty, ctx))
    }

    fn no_target_response(&'static self, ctx: &Context) -> Result<Response<Body>, Error> {
        let empty: Response<Body> = self.empty_response(self.no_targets_status)?;
        Ok(self.override_response(empty, ctx))
    }

    fn select_from_two_targets_response(
        &'static self,
        first_target_id: Option<String>,
        second_target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body> {
        if let Some(target_id) = first_target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                let resp = resp.unwrap();
                let ctx = ctx.with_response(&resp);
                self.override_response(resp, &ctx)
            } else {
                self.select_from_two_targets_response(None, second_target_id, responses, ctx)
            }
        } else if let Some(target_id) = second_target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                if let Some(resp) = resp {
                    let ctx = ctx.with_response(&resp);
                    self.override_response(resp, &ctx)
                } else {
                    self.no_target_response(ctx).unwrap()
                }
            } else {
                self.no_target_response(ctx).unwrap()
            }
        } else {
            self.no_target_response(ctx).unwrap()
        }
    }

    fn select_target_or_override_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body> {
        if let Some(target_id) = target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                let resp = resp.unwrap();
                let ctx = ctx.with_response(&resp);
                self.override_response(resp, &ctx)
            } else {
                self.override_empty_response(StatusCode::OK.into(), ctx)
                    .unwrap()
            }
        } else {
            self.override_empty_response(StatusCode::OK.into(), ctx)
                .unwrap()
        }
    }

    fn select_target_or_error_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Body> {
        if let Some(target_id) = target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                if let Some(resp) = resp {
                    let ctx = ctx.with_response(&resp);
                    self.override_response(resp, &ctx)
                } else {
                    self.no_target_response(ctx).unwrap()
                }
            } else {
                self.no_target_response(ctx).unwrap()
            }
        } else {
            self.no_target_response(ctx).unwrap()
        }
    }
}
