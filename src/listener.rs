use futures_util::future::join_all;
use http::{request::Parts, Error, HeaderValue};
use hyper::{
    header::HOST, http, Body, Client, Error as HyperError, HeaderMap, Request, Response,
    StatusCode, Uri,
};
use shellexpand::env_with_context_no_errors;
use std::{net::SocketAddr, collections::HashMap};
use tracing::debug;

use crate::{
    config::{
        headers::{HeaderTransform, HeaderTransformActon},
        listener::ListenerConfig,
        response::{OverrideConfig, ResponseStatus, ResponseStrategy},
        target::{TargetConfig, TargetOnErrorAction},
    },
    context::{Context, ContextMap},
};

pub struct Listener {}

impl Listener {
    pub async fn handler<'a>(
        cfg: &'static ListenerConfig,
        ctx: &'static Context<'a>,
        addr: SocketAddr,
        req: Request<Body>,
    ) -> Result<Response<Body>, Error> {
        // Verify is method allowed in the config
        if !cfg.is_method_allowed(req.method().as_ref()) {
            debug!("method `{}` rejected", req.method().to_string());
            return Response::builder()
                .status(StatusCode::METHOD_NOT_ALLOWED)
                .body(Body::empty());
        }

        // Prepare owned clonable body
        let (req_parts, req_body) = req.into_parts();
        let body_bytes = hyper::body::to_bytes(req_body).await.unwrap();
        // Add own context - listener + request
        let ctx = Listener::request_context(cfg, ctx, &addr, &req_parts);
        //debug!("request context: {:?}", ctx);

        // Prepare new headers
        let mut headers = req_parts.headers.clone();
        headers.remove(HOST);
        if let Some(transforms) = &cfg.headers {
            Listener::transform_headers(&mut headers, transforms, &ctx);
        }
        debug!("request headers: {:?}", headers);

        // Process targets
        debug!(
            "Listener={}, strategy={}",
            cfg.get_name(),
            cfg.response.strategy
        );

        let mut target_requests = vec![];
        let mut target_ctx = vec![];
        let mut target_ids: HashMap<String, usize> = HashMap::new();

        let http_client = Client::new();

        for (pos, target) in cfg.targets.iter().enumerate() {
            let ctx = Listener::target_context(&target, &ctx);
            let target_request_builder = Request::builder();
            // Set method
            let target_request_builder = target_request_builder.method(&req_parts.method);
            // Set uri
            let url = env_with_context_no_errors(&target.url, |v| ctx.get(&v.into()));
            let uri: Uri = url.parse()?;
            let mut target_request_builder = target_request_builder.uri(uri);
            // Prepare headers
            let mut headers = headers.clone();
            if let Some(transforms) = &target.headers {
                Listener::transform_headers(&mut headers, transforms, &ctx);
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
            let target_request: Request<Body> = if let Some(body) = &target.body {
                let body = env_with_context_no_errors(body, |v| ctx.get(&v.into()));
                target_request_builder.body(Body::from(body))?
            } else {
                target_request_builder.body(Body::from(body_bytes.clone()))?
            };

            // Put request to queue
            debug!("target `{}` request: {:?}", target.get_id(), target_request);
            target_requests.push(http_client.request(target_request));
            target_ctx.push(ctx);
            target_ids.insert(target.get_id(), pos);
        }

        // Get results
        let results: Vec<Result<Response<Body>, HyperError>> = join_all(target_requests).await;
        // Pre-process results
        let mut responses = vec![];
        for (pos, res) in results.into_iter().enumerate() {
            match res {
                Ok(resp) => {
                    debug!("OK: {:#?}", resp);
                    responses.push(Some(resp))
                }
                Err(e) => {
                    debug!("ERR: {:#?}", e);
                    let target = &cfg.targets[pos];
                    let resp = match target.on_error {
                        TargetOnErrorAction::Propagate => {
                            Some(Listener::build_on_error_response(e, &target.error_status))
                        }
                        TargetOnErrorAction::Status => {
                            Some(Listener::build_on_error_response(e, &target.error_status))
                        }
                        TargetOnErrorAction::Drop => None,
                    };
                    responses.push(resp);
                }
            }
        }

        // Select response according to strategy
        let resp = match &cfg.response.strategy {
            ResponseStrategy::AlwaysOverride => Listener::override_response(
                Response::new(Body::empty()),
                &ctx,
                &cfg.response.override_config,
            ),
            ResponseStrategy::AlwaysTargetId => todo!(),
            ResponseStrategy::OkThenFailed => todo!(),
            ResponseStrategy::OkThenTargetId => todo!(),
            ResponseStrategy::OkThenOverride => todo!(),
            ResponseStrategy::FailedThenOk => todo!(),
            ResponseStrategy::FailedThenTargetId => todo!(),
            ResponseStrategy::FailedThenOverride => todo!(),
            ResponseStrategy::ConditionalRouting => todo!(),
        };

        // Final response
        Ok(resp)
    }

    fn request_context<'a>(
        cfg: &'a ListenerConfig,
        ctx: &'a Context,
        addr: &'a SocketAddr,
        req: &'a Parts,
    ) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_LISTENER_NAME
        // CTX_REQUEST_SOURCE_IP
        // CTX_REQUEST_METHOD
        // CTX_REQUEST_HOST
        // CTX_REQUEST_PATH
        // CTX_REQUEST_QUERY
        own.insert("CTX_LISTENER_NAME".into(), cfg.get_name());
        own.insert("CTX_REQUEST_SOURCE_IP".into(), addr.ip().to_string());
        own.insert("CTX_REQUEST_METHOD".into(), req.method.to_string());
        own.insert("CTX_REQUEST_PATH".into(), req.uri.path().to_string());
        if let Some(host) = req.uri.host() {
            own.insert("CTX_REQUEST_HOST".into(), host.to_lowercase().to_string());
        }
        if let Some(query) = req.uri.query() {
            own.insert("CTX_REQUEST_QUERY".into(), query.to_lowercase().to_string());
        }

        // CTX_REQUEST_HEADERS_<UPPERCASE_HEADER_NAME>
        req.headers.iter().for_each(|(n, v)| {
            let n = n.as_str().to_uppercase().replace('-', "_");
            let v = v.to_str().unwrap_or("").to_string();
            own.insert(format!("CTX_REQUEST_HEADERS_{n}"), v);
        });

        ctx.with(own)
    }

    fn target_context<'a>(cfg: &'a TargetConfig, ctx: &'a Context) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_TARGET_ID
        // CTX_TARGET_HOST
        own.insert("CTX_TARGET_ID".into(), cfg.get_id());
        own.insert(
            "CTX_TARGET_HOST".into(),
            cfg.get_uri()
                .unwrap()
                .host()
                .unwrap()
                .to_lowercase()
                .to_string(),
        );

        ctx.with(own)
    }

    fn transform_headers(
        headers: &mut HeaderMap,
        transforms: &'static Vec<HeaderTransform>,
        ctx: &Context,
    ) {
        for transform in transforms {
            match &transform.action {
                HeaderTransformActon::Add(key) => {
                    if !headers.contains_key(key) {
                        let value = transform.value.as_ref().unwrap().as_str();
                        let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                        headers.insert(key.as_str(), HeaderValue::from_str(&value).unwrap());
                    }
                }
                HeaderTransformActon::Replace(key) => {
                    let value = transform.value.as_ref().unwrap().as_str();
                    let value = env_with_context_no_errors(value, |v| ctx.get(&v.into()));
                    headers.insert(key.as_str(), HeaderValue::from_str(&value).unwrap());
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

    fn build_on_error_response(e: HyperError, status: &Option<ResponseStatus>) -> Response<Body> {
        let resp = Response::builder();

        let resp = if let Some(status) = status {
            resp.status(status.get_code())
        } else if e.is_connect() || e.is_closed() {
            resp.status(502)
        } else if e.is_timeout() {
            resp.status(504)
        } else {
            resp.status(500)
        };

        resp.body(Body::empty()).unwrap()
    }

    fn override_response(
        resp: Response<Body>,
        ctx: &Context,
        cfg: &'static Option<OverrideConfig>,
    ) -> Response<Body> {
        if let Some(cfg) = cfg {
            let (resp_parts, resp_body) = resp.into_parts();
            let mut new_resp = Response::builder();

            // Set status
            new_resp = if let Some(status) = &cfg.status {
                new_resp.status(status.get_code())
            } else {
                new_resp.status(resp_parts.status)
            };

            // Prepare headers
            let mut headers = resp_parts.headers.clone();
            //let cfg_headers = cfg.headers.clone();
            if let Some(transforms) = &cfg.headers {
                Listener::transform_headers(&mut headers, transforms, ctx);
            }
            for (k, v) in &headers {
                new_resp = new_resp.header(k, v);
            }

            // Prepare body
            let cfg_body = cfg.body.clone();
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
}
