pub mod cli;
pub mod config;
pub mod context;
pub mod signal;

mod errors;
mod handler;
mod health_check;

use cli::CliConfig;
use config::{listener::ListenerConfig, AppConfig};
use context::{Context, RootEnvironment};
use futures_util::future::join_all;
use handler::RequestHandler;
use hyper::service::service_fn;
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
};
use signal::SignalHandler;
use std::{error::Error, sync::Arc};
use tokio::{
    net::TcpListener,
    select,
    task::{JoinHandle, JoinSet},
};
use tracing::{error, warn};

pub type HyperTaskJoinHandle = JoinHandle<Result<(), hyper::Error>>;

pub async fn run(
    cli_config: CliConfig,
    env_provider: impl RootEnvironment,
) -> Result<(), Box<dyn Error>> {
    let root_ctx = Arc::new(Context::root(env_provider));
    let app_config = AppConfig::new(&cli_config.config_path(), *root_ctx)?;
    let mut servers: Vec<HyperTaskJoinHandle> = vec![];

    for cfg in app_config.listeners().iter().map(Arc::new) {
        let listener = TcpListener::bind(&cfg.socket()).await?;

        let ctx = root_ctx.clone();
        let cfg = cfg.clone();

        let server = service_loop(listener, &ctx, &cfg);
        servers.push(tokio::spawn(server));
    }

    // Setup health check responder
    if let Some(port) = cli_config.health_check_port {
        servers.push(health_check::new(port, 5).await);
    }

    let _results = join_all(servers).await;

    Ok(())
}

async fn service_loop(
    listener: TcpListener,
    ctx: &'static Context<'static>,
    cfg: &'static ListenerConfig,
) -> Result<(), hyper::Error> {
    let mut join_set = JoinSet::new();

    let name = cfg.id();
    let handler = RequestHandler::new(cfg, ctx);
    let mut signal_handler = SignalHandler::new(name);

    loop {
        select! {
            biased;
            _ = signal_handler.wait() => {
                while (join_set.join_next().await).is_some() {};
                break
            },
            accepted = listener.accept() => {
                let (stream, addr) = match accepted {
                    Ok(x) => x,
                    Err(e) => {
                        warn!(error = %e, "failed to accept connection");
                        continue;
                    }
                };

                let serve_connection = async move {
                    let result = Builder::new(TokioExecutor::new())
                        .http1()
                        .timer(TokioTimer::default())
                        .header_read_timeout(cfg.timeout())
                        .serve_connection(
                            TokioIo::new(stream),
                            service_fn(move |req| handler.handle(addr, req)),
                        )
                        .await;

                    if let Err(e) = result {
                        error!(error = %e, "error serving request from {addr}");
                    }
                };

                join_set.spawn(serve_connection);
            }
        }
    }

    Ok(())
}
