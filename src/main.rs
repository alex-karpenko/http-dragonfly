mod cli;
mod config;
mod context;
mod errors;
mod handler;

use cli::CliConfig;
use config::{listener::ListenerConfig, AppConfig};
use context::{Context, RootOsEnvironment};
use futures_util::future::join_all;
use handler::RequestHandler;
use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Server,
};
use std::{convert::Infallible, error::Error, sync::Arc};
use tokio::{
    select,
    signal::unix::{signal, SignalKind},
};
use tracing::info;

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
    let cli_config = CliConfig::new();
    let root_ctx = Arc::new(Context::root(RootOsEnvironment::new(cli_config.env_mask())));
    let app_config = AppConfig::new(&cli_config.config_path(), *root_ctx)?;
    let mut servers = vec![];

    let listeners: Vec<Arc<&ListenerConfig>> =
        app_config.listeners().iter().map(Arc::new).collect();

    for cfg in listeners {
        let server = Server::bind(&cfg.socket());
        let server = server.http1_header_read_timeout(cfg.timeout());

        let name = cfg.id();
        let ctx = root_ctx.clone();
        let cfg = cfg.clone();
        let handler = RequestHandler::new(*cfg, *ctx);

        let make_service = make_service_fn(move |conn: &AddrStream| {
            let addr = conn.remote_addr();
            let service = service_fn(move |req| handler.handle(addr, req));

            async move { Ok::<_, Infallible>(service) }
        });

        let server = server
            .serve(make_service)
            .with_graceful_shutdown(shutdown_signal(name));

        servers.push(server);
    }

    let _results = join_all(servers).await;

    Ok(())
}

/// Shutdown signal handler
///
/// Subscribes on and waits for one of the terminate/interrupt/quit/hangup signals
async fn shutdown_signal(listener_name: String) {
    let mut terminate = signal(SignalKind::terminate())
        .expect("{listener_name}: unable to install TERM signal handler");
    let mut interrupt = signal(SignalKind::interrupt())
        .expect("{listener_name}: unable to install INT signal handler");
    let mut quit =
        signal(SignalKind::quit()).expect("{listener_name}: unable to install QUIT signal handler");
    let mut hangup = signal(SignalKind::hangup())
        .expect("{listener_name}: unable to install HANGUP signal handler");

    let sig = select! {
        _ = terminate.recv() => "TERM",
        _ = interrupt.recv() => "INT",
        _ = quit.recv() => "QUIT",
        _ = hangup.recv() => "HANGUP",
    };

    info!("{listener_name}: {sig} signal has been received, shutting down");
}
