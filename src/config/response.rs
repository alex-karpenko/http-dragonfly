use super::{
    headers::{HeaderTransform, HeadersTransformator},
    ConfigValidator,
};
use crate::{
    config,
    context::Context,
    handler::{ResponseResult, ResponsesMap},
};
use http_body_util::Full;
use hyper::{body::Bytes, header::CONTENT_LENGTH, http::Error, Response, StatusCode};
use regex::Regex;
use serde::{Deserialize, Serialize};
use shellexpand::env_with_context_no_errors;
use tracing::debug;

pub type ResponseStatus = u16;

const UNABLE_TO_CREATE_RESPONSE_ERROR: &str = "unable to create response, looks like a BUG";

#[derive(Deserialize, Debug, Serialize)]
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

#[derive(Deserialize, Debug, Serialize)]
#[serde(deny_unknown_fields)]
pub struct OverrideConfig {
    status: Option<ResponseStatus>,
    body: Option<String>,
    headers: Option<Vec<HeaderTransform>>,
}

impl ConfigValidator for ResponseConfig {
    fn validate(&self) -> Result<(), config::ConfigError> {
        Ok(())
    }
}

pub trait ResponseBehavior {
    fn target_selector(&self) -> &Option<String>;
    fn override_response(
        &'static self,
        resp: Response<Full<Bytes>>,
        ctx: &Context,
    ) -> Response<Full<Bytes>>;
    fn find_first_response(
        &self,
        responses: &ResponsesMap,
        response_kind: ResponseKind,
    ) -> Option<String>;
    fn error_response(
        &self,
        e: ResponseResult,
        status: &Option<ResponseStatus>,
    ) -> Response<Full<Bytes>>;
    fn empty_response(&self, status: ResponseStatus) -> Result<Response<Full<Bytes>>, Error>;
    fn override_empty_response(
        &'static self,
        status: ResponseStatus,
        ctx: &Context,
    ) -> Result<Response<Full<Bytes>>, Error>;
    fn no_target_response(&'static self, ctx: &Context) -> Result<Response<Full<Bytes>>, Error>;
    fn select_from_two_targets_response(
        &'static self,
        first_target_id: Option<String>,
        second_target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>>;
    fn select_target_or_override_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>>;
    fn select_target_or_error_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>>;
}

#[derive(Debug)]
pub enum ResponseKind {
    Ok,
    Failed,
}

impl ResponseBehavior for ResponseConfig {
    fn target_selector(&self) -> &Option<String> {
        &self.target_selector
    }

    fn override_response(
        &'static self,
        resp: Response<Full<Bytes>>,
        ctx: &Context,
    ) -> Response<Full<Bytes>> {
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
            let body: Full<Bytes> = if let Some(body) = &cfg.body {
                // Remove Content-length header since it's incorrect now
                headers.remove(CONTENT_LENGTH);
                let body: String = env_with_context_no_errors(&body, |v| ctx.get(&v.into())).into();
                Full::from(body)
            } else {
                resp_body
            };

            // Final response
            new_resp.body(body).expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
        } else {
            resp
        }
    }

    fn find_first_response(
        &self,
        responses: &ResponsesMap,
        response_kind: ResponseKind,
    ) -> Option<String> {
        debug!("looking for {:?}", response_kind);

        let re = Regex::new(&self.failed_status_regex).unwrap_or_else(|_| {
            panic!(
                "unable parse regex expression: {}",
                self.failed_status_regex
            )
        });
        for key in responses.keys() {
            let (resp, _) = responses
                .get(key)
                .expect("unable to get header value by key, looks like a BUG");
            if let Some(resp) = resp {
                let status: String = resp.status().to_string();
                let is_failed = re.is_match(&status);
                match response_kind {
                    ResponseKind::Ok => {
                        if !is_failed {
                            debug!("found target id={}", key);
                            return Some(key.into());
                        }
                    }
                    ResponseKind::Failed => {
                        if is_failed {
                            debug!("found target id={}", key);
                            return Some(key.into());
                        }
                    }
                }
            }
        }

        debug!("not found any target");
        None
    }

    fn error_response(
        &self,
        e: ResponseResult,
        status: &Option<ResponseStatus>,
    ) -> Response<Full<Bytes>> {
        let resp = Response::builder();
        let resp = if let Some(status) = status.to_owned() {
            resp.status(status)
        } else {
            match e {
                ResponseResult::HyperError(e) => {
                    if e.is_connect() {
                        resp.status(StatusCode::BAD_GATEWAY)
                    } else {
                        resp.status(StatusCode::INTERNAL_SERVER_ERROR)
                    }
                }
                ResponseResult::Timeout => resp.status(StatusCode::GATEWAY_TIMEOUT),
                _ => {
                    panic!("Looks like a BUG!")
                }
            }
        };

        resp.body(Full::from(Bytes::new()))
            .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
    }

    fn empty_response(&self, status: ResponseStatus) -> Result<Response<Full<Bytes>>, Error> {
        Response::builder()
            .status(status)
            .body(Full::from(Bytes::new()))
    }

    fn override_empty_response(
        &'static self,
        status: ResponseStatus,
        ctx: &Context,
    ) -> Result<Response<Full<Bytes>>, Error> {
        let empty = self.empty_response(status)?;
        Ok(self.override_response(empty, ctx))
    }

    fn no_target_response(&'static self, ctx: &Context) -> Result<Response<Full<Bytes>>, Error> {
        let empty: Response<Full<Bytes>> = self.empty_response(self.no_targets_status)?;
        Ok(self.override_response(empty, ctx))
    }

    fn select_from_two_targets_response(
        &'static self,
        first_target_id: Option<String>,
        second_target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>> {
        if let Some(target_id) = first_target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                let resp = resp.expect(UNABLE_TO_CREATE_RESPONSE_ERROR);
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
                    self.no_target_response(ctx)
                        .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
                }
            } else {
                self.no_target_response(ctx)
                    .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
            }
        } else {
            self.no_target_response(ctx)
                .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
        }
    }

    fn select_target_or_override_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>> {
        if let Some(target_id) = target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                let resp = resp.expect(UNABLE_TO_CREATE_RESPONSE_ERROR);
                let ctx = ctx.with_response(&resp);
                self.override_response(resp, &ctx)
            } else {
                self.override_empty_response(StatusCode::OK.into(), ctx)
                    .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
            }
        } else {
            self.override_empty_response(StatusCode::OK.into(), ctx)
                .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
        }
    }

    fn select_target_or_error_response(
        &'static self,
        target_id: Option<String>,
        responses: &mut ResponsesMap,
        ctx: &Context,
    ) -> Response<Full<Bytes>> {
        if let Some(target_id) = target_id {
            if let Some((resp, ctx)) = responses.remove(&target_id) {
                if let Some(resp) = resp {
                    let ctx = ctx.with_response(&resp);
                    self.override_response(resp, &ctx)
                } else {
                    self.no_target_response(ctx)
                        .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
                }
            } else {
                self.no_target_response(ctx)
                    .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
            }
        } else {
            self.no_target_response(ctx)
                .expect(UNABLE_TO_CREATE_RESPONSE_ERROR)
        }
    }
}
