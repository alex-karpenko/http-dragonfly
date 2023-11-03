use futures_util::future::join_all;
use http::{request::Parts, Error, HeaderValue};
use hyper::{
    client::HttpConnector, header::HOST, http, Body, Client, Error as HyperError, Request,
    Response, StatusCode, Uri,
};
use hyper_tls::HttpsConnector;

use shellexpand::env_with_context_no_errors;
use std::{collections::HashMap, net::SocketAddr};
use tracing::debug;

use crate::{
    config::{
        headers::HeadersTransformator,
        listener::ListenerConfig,
        response::{ResponseBehavior, ResponseStatus},
        strategy::ResponseStrategy,
        target::{TargetBehavior, TargetConditionConfig, TargetConfig, TargetOnErrorAction},
    },
    context::{Context, ContextMap},
};

pub type ResponsesMap<'a> = HashMap<String, (Option<Response<Body>>, &'a Context<'a>)>;

#[derive(Clone, Copy)]
pub struct RequestHandler {
    pub listener_cfg: &'static ListenerConfig,
    pub root_ctx: &'static Context<'static>,
}

impl RequestHandler {
    pub fn new(cfg: &'static ListenerConfig, ctx: &'static Context) -> Self {
        Self {
            listener_cfg: cfg,
            root_ctx: ctx,
        }
    }

    pub async fn handle<'a>(
        self,
        addr: SocketAddr,
        req: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        // Verify is method allowed in the config
        if !self.listener_cfg.is_method_allowed(req.method().as_ref()) {
            debug!("method `{}` rejected", req.method().to_string());
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Body::empty());
        }

        // Prepare owned body
        let (req_parts, req_body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(req_body).await.unwrap();
        // Add own context - listener + request
        let ctx = self.request_context(&addr, &req_parts);
        //debug!("request context: {:?}", ctx);

        // Prepare new headers
        let mut headers = req_parts.headers.clone();
        headers.remove(HOST);
        if let Some(transforms) = self.listener_cfg.headers() {
            transforms.transform(&mut headers, &ctx)
        }
        debug!("request headers: {:?}", headers);

        // Process targets
        debug!(
            "Listener={}, strategy={}",
            self.listener_cfg.name(),
            self.listener_cfg.strategy()
        );

        let mut target_requests = vec![];
        let mut target_ctx = vec![];
        let mut target_ids = vec![];

        let mut targets: Vec<&TargetConfig> = vec![];
        let mut conditional_target_id: Option<String> = None;
        let response_config = self.listener_cfg.response();

        // Verify conditions
        for target in self.listener_cfg.targets() {
            match &self.listener_cfg.strategy() {
                // Special flow in case of conditional routing
                ResponseStrategy::ConditionalRouting => {
                    match target.condition().as_ref().unwrap() {
                        // Always insert default into empty targets list
                        TargetConditionConfig::Default => {
                            if targets.is_empty() {
                                targets.push(target)
                            }
                        }
                        TargetConditionConfig::Filter(_) => {
                            if target.check_condition(&ctx, &req_parts, &body_bytes) {
                                if targets.is_empty() {
                                    targets.push(target)
                                } else if matches!(
                                    targets[0].condition().as_ref().unwrap(),
                                    TargetConditionConfig::Default
                                ) {
                                    // Replace default by this target
                                    targets.pop();
                                    targets.push(target);
                                } else {
                                    // Error - more than one target has true condition
                                    let empty = Response::builder()
                                        .status(response_config.no_targets_status())
                                        .body(Body::empty())?;
                                    return Ok(response_config.override_response(empty, &ctx));
                                }
                            }
                        }
                    };
                    if !targets.is_empty() {
                        conditional_target_id = Some(targets[0].id())
                    }
                }
                // Any other strategy
                _ => {
                    if let Some(condition) = target.condition().as_ref() {
                        match condition {
                            TargetConditionConfig::Default => targets.push(target),
                            TargetConditionConfig::Filter(_) => {
                                if target.check_condition(&ctx, &req_parts, &body_bytes) {
                                    targets.push(target)
                                }
                            }
                        }
                    } else {
                        targets.push(target);
                    }
                }
            }
        }

        for target in targets.iter() {
            let ctx = self.target_context(target, &ctx);
            let target_request_builder = Request::builder();
            // Set method
            let target_request_builder = target_request_builder.method(&req_parts.method);
            // Set uri
            let url = env_with_context_no_errors(target.url(), |v| ctx.get(&v.into()));
            let uri: Uri = url.parse()?;
            let mut target_request_builder = target_request_builder.uri(uri);
            // Prepare headers
            let mut headers = headers.clone();
            if let Some(transforms) = &target.headers() {
                transforms.transform(&mut headers, &ctx);
            }
            // Add Host header if empty
            if !headers.contains_key(HOST) {
                debug!("add host header");
                let host: Uri = url.parse()?;
                let host = host.host().unwrap();
                headers.insert(HOST, HeaderValue::from_str(host)?);
            }
            // Insert all headers into request
            for (k, v) in &headers {
                target_request_builder = target_request_builder.header(k, v);
            }
            // Finalize request with body
            let target_request: Request<Body> = if let Some(body) = &target.body() {
                let body = env_with_context_no_errors(body, |v| ctx.get(&v.into()));
                target_request_builder.body(Body::from(body))?
            } else {
                target_request_builder.body(Body::from(body_bytes.clone()))?
            };

            // Put request to queue
            debug!("target `{}` request: {:?}", target.id(), target_request);

            // Make a connector
            let mut http_connector = HttpConnector::new();
            http_connector.set_connect_timeout(Some(target.timeout()));
            http_connector.enforce_http(false);
            let https_connector = HttpsConnector::new_with_connector(http_connector);
            let http_client = Client::builder().build(https_connector);

            target_requests.push(http_client.request(target_request));
            target_ctx.push(ctx);
            target_ids.push(target.id());
        }

        // Get results
        let results: Vec<Result<Response<Body>, HyperError>> = join_all(target_requests).await;
        // Pre-process results
        let mut responses: ResponsesMap = ResponsesMap::new();
        for (pos, res) in results.into_iter().enumerate() {
            match res {
                Ok(resp) => {
                    debug!("OK: {:#?}", resp);
                    responses.insert(target_ids[pos].clone(), (Some(resp), &target_ctx[pos]));
                }
                Err(e) => {
                    debug!("ERR: {:#?}", e);
                    let target = targets[pos];
                    let resp = match target.on_error() {
                        TargetOnErrorAction::Propagate => {
                            Some(self.build_error_response(e, &target.error_status()))
                        }
                        TargetOnErrorAction::Status => {
                            Some(self.build_error_response(e, &target.error_status()))
                        }
                        TargetOnErrorAction::Drop => None,
                    };
                    responses.insert(target_ids[pos].clone(), (resp, &target_ctx[pos]));
                }
            }
        }

        // Select/create response according to strategy
        let ok_target_id = response_config.find_first_response(&responses, false);
        let failed_target_id = response_config.find_first_response(&responses, true);
        let selector_target_id = response_config.target_selector().clone();
        let resp = match &self.listener_cfg.strategy() {
            ResponseStrategy::AlwaysOverride => {
                let empty = Response::new(Body::empty());
                response_config.override_response(empty, &ctx)
            }
            ResponseStrategy::AlwaysTargetId => {
                let target_id = selector_target_id.unwrap();
                let (resp, ctx) = responses.remove(&target_id).unwrap();
                if let Some(resp) = resp {
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else {
                    let empty = Response::builder()
                        .status(response_config.no_targets_status())
                        .body(Body::empty())?;
                    response_config.override_response(empty, ctx)
                }
            }
            ResponseStrategy::OkThenOverride => {
                if let Some(ok_target_id) = ok_target_id {
                    let (resp, ctx) = responses.remove(&ok_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else {
                    let empty = Response::new(Body::empty());
                    response_config.override_response(empty, &ctx)
                }
            }
            ResponseStrategy::FailedThenOverride => {
                if let Some(failed_target_id) = failed_target_id {
                    let (resp, ctx) = responses.remove(&failed_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else {
                    let empty = Response::new(Body::empty());
                    response_config.override_response(empty, &ctx)
                }
            }
            ResponseStrategy::OkThenTargetId => {
                if let Some(ok_target_id) = ok_target_id {
                    let (resp, ctx) = responses.remove(&ok_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else {
                    let target_id = selector_target_id.unwrap();
                    let (resp, ctx) = responses.remove(&target_id).unwrap();
                    if let Some(resp) = resp {
                        let ctx = self.response_context(&resp, ctx);
                        response_config.override_response(resp, &ctx)
                    } else {
                        let empty = Response::builder()
                            .status(response_config.no_targets_status())
                            .body(Body::empty())?;
                        response_config.override_response(empty, ctx)
                    }
                }
            }
            ResponseStrategy::FailedThenTargetId => {
                if let Some(failed_target_id) = failed_target_id {
                    let (resp, ctx) = responses.remove(&failed_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else {
                    let target_id = selector_target_id.unwrap();
                    let (resp, ctx) = responses.remove(&target_id).unwrap();
                    if let Some(resp) = resp {
                        let ctx = self.response_context(&resp, ctx);
                        response_config.override_response(resp, &ctx)
                    } else {
                        let empty = Response::builder()
                            .status(response_config.no_targets_status())
                            .body(Body::empty())?;
                        response_config.override_response(empty, ctx)
                    }
                }
            }
            ResponseStrategy::OkThenFailed => {
                if let Some(ok_target_id) = ok_target_id {
                    let (resp, ctx) = responses.remove(&ok_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else if let Some(failed_target_id) = failed_target_id {
                    let (resp, ctx) = responses.remove(&failed_target_id).unwrap();
                    if let Some(resp) = resp {
                        let ctx = self.response_context(&resp, ctx);
                        response_config.override_response(resp, &ctx)
                    } else {
                        let empty = Response::builder()
                            .status(response_config.no_targets_status())
                            .body(Body::empty())?;
                        response_config.override_response(empty, ctx)
                    }
                } else {
                    let empty = Response::builder()
                        .status(response_config.no_targets_status())
                        .body(Body::empty())?;
                    response_config.override_response(empty, &ctx)
                }
            }
            ResponseStrategy::FailedThenOk => {
                if let Some(failed_target_id) = failed_target_id {
                    let (resp, ctx) = responses.remove(&failed_target_id).unwrap();
                    let resp = resp.unwrap();
                    let ctx = self.response_context(&resp, ctx);
                    response_config.override_response(resp, &ctx)
                } else if let Some(ok_target_id) = ok_target_id {
                    let (resp, ctx) = responses.remove(&ok_target_id).unwrap();
                    if let Some(resp) = resp {
                        let ctx = self.response_context(&resp, ctx);
                        response_config.override_response(resp, &ctx)
                    } else {
                        let empty = Response::builder()
                            .status(response_config.no_targets_status())
                            .body(Body::empty())?;
                        response_config.override_response(empty, ctx)
                    }
                } else {
                    let empty = Response::builder()
                        .status(response_config.no_targets_status())
                        .body(Body::empty())?;
                    response_config.override_response(empty, &ctx)
                }
            }
            ResponseStrategy::ConditionalRouting => {
                if let Some(target_id) = conditional_target_id {
                    let (resp, ctx) = responses.remove(&target_id).unwrap();
                    if let Some(resp) = resp {
                        let ctx = self.response_context(&resp, ctx);
                        response_config.override_response(resp, &ctx)
                    } else {
                        let empty = Response::builder()
                            .status(response_config.no_targets_status())
                            .body(Body::empty())?;
                        response_config.override_response(empty, ctx)
                    }
                } else {
                    let empty = Response::builder()
                        .status(response_config.no_targets_status())
                        .body(Body::empty())?;
                    response_config.override_response(empty, &ctx)
                }
            }
        };

        // Final response
        Ok(resp)
    }

    fn request_context<'a>(&'a self, addr: &'a SocketAddr, req: &'a Parts) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_LISTENER_NAME
        // CTX_REQUEST_SOURCE_IP
        // CTX_REQUEST_METHOD
        // CTX_REQUEST_HOST
        // CTX_REQUEST_PATH
        // CTX_REQUEST_QUERY
        own.insert("CTX_LISTENER_NAME".into(), self.listener_cfg.name());
        own.insert("CTX_REQUEST_SOURCE_IP".into(), addr.ip().to_string());
        own.insert("CTX_REQUEST_METHOD".into(), req.method.to_string());
        own.insert("CTX_REQUEST_PATH".into(), req.uri.path().to_string());
        if let Some(host) = req.uri.host() {
            own.insert("CTX_REQUEST_HOST".into(), host.to_lowercase());
        }
        if let Some(query) = req.uri.query() {
            own.insert("CTX_REQUEST_QUERY".into(), query.to_lowercase());
        }

        // CTX_REQUEST_HEADERS_<UPPERCASE_HEADER_NAME>
        req.headers.iter().for_each(|(n, v)| {
            let n = n.as_str().to_uppercase().replace('-', "_");
            let v = v.to_str().unwrap_or("").to_string();
            own.insert(format!("CTX_REQUEST_HEADERS_{n}"), v);
        });

        self.root_ctx.with(own)
    }

    fn target_context<'a>(&'a self, cfg: &'a TargetConfig, ctx: &'a Context) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_TARGET_ID
        // CTX_TARGET_HOST
        own.insert("CTX_TARGET_ID".into(), cfg.id());
        own.insert("CTX_TARGET_HOST".into(), cfg.host());

        ctx.with(own)
    }

    fn response_context<'a>(&'a self, resp: &Response<Body>, ctx: &'a Context) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_RESPONSE_HEADERS_<UPPERCASE_HEADER_NAME>
        // CTX_RESPONSE_STATUS
        own.insert("CTX_RESPONSE_STATUS".into(), resp.status().to_string());
        resp.headers().iter().for_each(|(n, v)| {
            let n = n.as_str().to_uppercase().replace('-', "_");
            let v = v.to_str().unwrap_or("").to_string();
            own.insert(format!("CTX_RESPONSE_HEADERS_{n}"), v);
        });

        ctx.with(own)
    }

    fn build_error_response(
        &self,
        e: HyperError,
        status: &Option<ResponseStatus>,
    ) -> Response<Body> {
        let resp = Response::builder();

        let resp = if let Some(status) = status.to_owned() {
            resp.status(status)
        } else if e.is_connect() || e.is_closed() {
            resp.status(502)
        } else if e.is_timeout() {
            resp.status(504)
        } else {
            resp.status(500)
        };

        resp.body(Body::empty()).unwrap()
    }
}
