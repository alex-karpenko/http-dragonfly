use crate::{signal::SignalHandler, HyperTaskJoinHandle};
use core::time::Duration;
use http_body_util::Full;
use hyper::{body::Bytes, http, service::service_fn, Response};
use hyper_util::{
    rt::{TokioExecutor, TokioIo, TokioTimer},
    server::conn::auto::Builder,
};
use std::net::{Ipv4Addr, SocketAddr};
use tokio::{net::TcpListener, select, task::JoinSet};
use tracing::{debug, info, warn};

/// Health check handler
///
/// Listens to health-check port and responds 200 OK to any GET request
async fn handle(addr: SocketAddr) -> Result<Response<Full<Bytes>>, http::Error> {
    debug!("health check, from={}", addr);
    Ok(Response::new(Full::from(Bytes::from("OK\n"))))
}

/// Health check handler builder
pub async fn new(port: u16, timeout_sec: u64) -> HyperTaskJoinHandle {
    info!("Creating health check handler on *:{}", port);

    let ip = Ipv4Addr::new(0, 0, 0, 0);
    let socket = SocketAddr::new(ip.into(), port);
    let listener = TcpListener::bind(&socket)
        .await
        .expect("unable to create health check listener");
    let mut signal_handler = SignalHandler::new("health");
    let mut join_set = JoinSet::new();

    let server = async move {
        loop {
            select! {
                biased;
                _ = signal_handler.wait() => {
                    while (join_set.join_next().await).is_some() {}
                    break
                },
                accepted = listener.accept() => {
                    let (stream, addr) = match accepted {
                        Ok(x) => x,
                        Err(e) => {
                            warn!(error = %e, "failed to accept health check connection");
                            continue;
                        }
                    };

                    let serve_connection = async move {
                        let result = Builder::new(TokioExecutor::new())
                            .http1()
                            .timer(TokioTimer::default())
                            .header_read_timeout(Duration::from_secs(timeout_sec))
                            .serve_connection(TokioIo::new(stream), service_fn(move |_| handle(addr)))
                            .await;

                        if let Err(e) = result {
                            debug!(error = %e, "error serving health check request from {addr}");
                        }
                    };

                    join_set.spawn(serve_connection);
                }
            }
        }

        Ok(())
    };

    tokio::spawn(server)
}
