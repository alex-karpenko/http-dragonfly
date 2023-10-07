mod cli;
mod config;
mod context;
mod errors;
mod listener;

use cli::CliConfig;
use config::{listener::ListenerConfig, AppConfig};
use context::Context;
use futures_util::future::join_all;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use listener::Listener;
use std::{convert::Infallible, error::Error, sync::Arc};

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let ctx = Arc::new(Context::root());
    let app_config = AppConfig::new(&cli_config.config, *ctx)?;
    let mut servers = vec![];

    let listeners: Vec<Arc<&ListenerConfig>> = app_config.listeners.iter().map(Arc::new).collect();

    for cfg in listeners {
        let server = Server::bind(&cfg.get_socket());

        let ctx = ctx.clone();
        let make_service = make_service_fn(move |conn: &AddrStream| {
            let addr = conn.remote_addr();
            let cfg = cfg.clone();
            let ctx = ctx.clone();
            let service = service_fn(move |req| Listener::handler(*cfg, *ctx, addr, req));

            async move { Ok::<_, Infallible>(service) }
        });

        let server = server.serve(make_service);

        servers.push(server);
    }

    let _results = join_all(servers).await;

    Ok(())
}
