pub mod cli;
pub mod config;
pub mod context;
pub mod signal;

mod aws_auth;
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
use rustls::crypto::aws_lc_rs;
use signal::SignalHandler;
use std::sync::Arc;
use tokio::{
    net::TcpListener,
    select,
    task::{JoinHandle, JoinSet},
};
use tracing::{debug, error, warn};

pub type HyperTaskJoinHandle = JoinHandle<Result<(), anyhow::Error>>;

pub async fn run(
    cli_config: CliConfig,
    env_provider: impl RootEnvironment,
) -> Result<(), anyhow::Error> {
    let _ = aws_lc_rs::default_provider().install_default();

    let root_ctx = Arc::new(Context::root(env_provider));
    let app_config = AppConfig::new(cli_config.config_path(), *root_ctx)?;
    aws_auth::init(app_config).await?;
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
        servers.push(health_check::new(port, cli_config.health_check_timeout).await);
    }

    let _results = join_all(servers).await;

    Ok(())
}

async fn service_loop(
    listener: TcpListener,
    ctx: &'static Context<'static>,
    cfg: &'static ListenerConfig,
) -> Result<(), anyhow::Error> {
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
                        if is_client_header_timeout(&*e) {
                            debug!(error = %e, "error serving request from {addr}");
                        } else {
                            error!(error = %e, "error serving request from {addr}");
                        }
                    }
                };

                join_set.spawn(serve_connection);
            }
        }
    }

    Ok(())
}

/// Idle clients (e.g. browsers holding preconnected sockets) hitting `header_read_timeout`
/// are a client-side condition, not a server fault.
fn is_client_header_timeout(e: &(dyn std::error::Error + Send + Sync + 'static)) -> bool {
    e.downcast_ref::<hyper::Error>()
        .is_some_and(hyper::Error::is_timeout)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::{body::Incoming, Request, Response};
    use std::{
        convert::Infallible,
        io::{Read, Write},
        time::Duration,
    };

    type BoxError = Box<dyn std::error::Error + Send + Sync>;

    /// Accepts one connection with the same http1 builder settings as `service_loop`,
    /// runs `client` against it and returns the connection result.
    async fn serve_one(client: impl FnOnce(std::net::TcpStream) + Send + 'static) -> BoxError {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let client = tokio::task::spawn_blocking(move || {
            client(std::net::TcpStream::connect(addr).unwrap());
        });

        let (stream, _) = listener.accept().await.unwrap();
        let result = Builder::new(TokioExecutor::new())
            .http1()
            .timer(TokioTimer::default())
            .header_read_timeout(Duration::from_millis(100))
            .serve_connection(
                TokioIo::new(stream),
                service_fn(|_: Request<Incoming>| async {
                    Ok::<_, Infallible>(Response::new(String::new()))
                }),
            )
            .await;

        client.await.unwrap();
        result.expect_err("connection should fail")
    }

    #[tokio::test]
    async fn idle_keep_alive_client_is_header_timeout() {
        // Browser-like behaviour: complete one request, then keep the connection open without
        // sending the next one. hyper re-arms header_read_timeout for the idle keep-alive connection.
        let err = serve_one(|mut stream| {
            stream
                .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
                .unwrap();
            let mut buf = [0u8; 1024];
            let n = stream.read(&mut buf).unwrap();
            assert!(buf[..n].starts_with(b"HTTP/1.1 200"));
            std::thread::sleep(Duration::from_millis(500));
        })
        .await;

        assert!(is_client_header_timeout(&*err), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn partial_headers_is_header_timeout() {
        let err = serve_one(|mut stream| {
            stream.write_all(b"GET / HTTP/1.1\r\nHost: loc").unwrap();
            std::thread::sleep(Duration::from_millis(500));
        })
        .await;

        assert!(is_client_header_timeout(&*err), "unexpected error: {err}");
    }

    #[tokio::test]
    async fn malformed_request_is_not_header_timeout() {
        let err = serve_one(|mut stream| {
            stream.write_all(b"NOT HTTP AT ALL\r\n\r\n").unwrap();
            std::thread::sleep(Duration::from_millis(50));
        })
        .await;

        assert!(!is_client_header_timeout(&*err), "unexpected error: {err}");
    }

    #[test]
    fn non_hyper_error_is_not_header_timeout() {
        let err: BoxError = Box::new(std::io::Error::from(std::io::ErrorKind::TimedOut));
        assert!(!is_client_header_timeout(&*err));
    }
}
