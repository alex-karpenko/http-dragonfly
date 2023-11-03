use hyper::{http::request::Parts, Body, Response};
use once_cell::sync::OnceCell;
use regex::Regex;
use std::collections::HashMap;
use std::env;
use std::net::SocketAddr;

use crate::config::target::TargetConfig;

const CTX_APP_NAME: &str = env!("CARGO_PKG_NAME");
const CTX_APP_VERSION: &str = env!("CARGO_PKG_VERSION");

pub type ContextMap = HashMap<String, String>;

#[derive(Debug)]
pub struct Context<'a> {
    own: ContextMap,
    parent: Option<&'a Context<'a>>,
}

impl<'a> Context<'a> {
    pub fn root(root_env: impl RootEnvironment) -> &'static Context<'a> {
        static ROOT_CONTEXT: OnceCell<Context> = OnceCell::new();

        let ctx = ROOT_CONTEXT.get_or_init(|| {
            let mut ctx = ContextMap::new();

            ctx.insert("CTX_APP_NAME".into(), CTX_APP_NAME.into());
            ctx.insert("CTX_APP_VERSION".into(), CTX_APP_VERSION.into());
            ctx.extend(root_env.get_environment());

            Context {
                own: ctx,
                parent: None,
            }
        });

        ctx
    }

    pub fn with(&self, own: ContextMap) -> Context {
        Context {
            own,
            parent: Some(self),
        }
    }

    pub fn get(&self, var: &String) -> Option<&String> {
        // Try own context
        if let Some(value) = self.own.get(var) {
            Some(value)
            // or get parent if it's
        } else if let Some(parent) = &self.parent {
            // and try parent
            if let Some(value) = parent.get(var) {
                Some(value)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub fn iter(&'a self) -> Box<dyn Iterator<Item = (&'a String, &'a String)> + 'a> {
        let iter = Box::new(self.own.iter());
        Box::new(Iter {
            ctx: self,
            iter,
            finished: false,
        })
    }

    pub fn with_request(
        &'a self,
        addr: &'a SocketAddr,
        req: &'a Parts,
        listener_name: String,
    ) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_LISTENER_NAME
        // CTX_REQUEST_SOURCE_IP
        // CTX_REQUEST_METHOD
        // CTX_REQUEST_HOST
        // CTX_REQUEST_PATH
        // CTX_REQUEST_QUERY
        own.insert("CTX_LISTENER_NAME".into(), listener_name);
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

        self.with(own)
    }

    pub fn with_target(&'a self, cfg: &'a TargetConfig) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_TARGET_ID
        // CTX_TARGET_HOST
        own.insert("CTX_TARGET_ID".into(), cfg.id());
        own.insert("CTX_TARGET_HOST".into(), cfg.host());

        self.with(own)
    }

    pub fn with_response(&'a self, resp: &Response<Body>) -> Context<'a> {
        let mut own = ContextMap::new();

        // CTX_RESPONSE_HEADERS_<UPPERCASE_HEADER_NAME>
        // CTX_RESPONSE_STATUS
        own.insert("CTX_RESPONSE_STATUS".into(), resp.status().to_string());
        resp.headers().iter().for_each(|(n, v)| {
            let n = n.as_str().to_uppercase().replace('-', "_");
            let v = v.to_str().unwrap_or("").to_string();
            own.insert(format!("CTX_RESPONSE_HEADERS_{n}"), v);
        });

        self.with(own)
    }
}

pub struct Iter<'a> {
    ctx: &'a Context<'a>,
    iter: Box<dyn Iterator<Item = (&'a String, &'a String)> + 'a>,
    finished: bool,
}

impl<'a> Iterator for Iter<'a> {
    type Item = (&'a String, &'a String);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(next) = self.iter.next() {
            Some(next)
        } else if self.ctx.parent.is_none() {
            None
        } else if !self.finished {
            self.finished = true;
            self.iter = self.ctx.parent.unwrap().iter();
            self.iter.next()
        } else {
            None
        }
    }
}

pub trait RootEnvironment {
    fn get_environment(&self) -> ContextMap;
}

pub struct RootOsEnvironment<'a> {
    env_mask_regex: &'a str,
}

impl<'a> RootEnvironment for RootOsEnvironment<'a> {
    fn get_environment(&self) -> ContextMap {
        let mut env_ctx = ContextMap::new();
        let re = Regex::new(self.env_mask_regex).expect("invalid environment filter regex");
        env::vars().for_each(|(k, v)| {
            if re.is_match(&k) {
                env_ctx.insert(k, v);
            }
        });

        env_ctx
    }
}

impl<'a> RootOsEnvironment<'a> {
    pub fn new(env_mask_regex: &'a str) -> Self {
        Self { env_mask_regex }
    }
}
