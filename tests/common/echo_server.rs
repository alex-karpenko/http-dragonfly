use http_body_util::{BodyExt, Full};
use http_dragonfly::signal::SignalHandler;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Error, Request, Response};
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto::Builder;
use std::net::SocketAddr;
use std::time::Duration;
use std::{convert::Infallible, net::Ipv4Addr};
use tokio::net::TcpListener;
use tokio::select;
use tokio::task::JoinSet;
use tracing::{debug, error, info, warn};

pub async fn echo_server(port: u16) -> Result<(), Error> {
    info!("create echo server on port: {}", port);

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
                    while (join_set.join_next().await).is_some() {};
                    break;
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
                            .serve_connection(TokioIo::new(stream), service_fn(handle_request))
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
    };

    tokio::task::spawn(server).await.unwrap()
}

async fn handle_request(req: Request<Incoming>) -> Result<Response<Full<Bytes>>, Infallible> {
    let mut response = Response::new(Full::from(""));
    let echo_headers = response.headers_mut();
    let headers = req.headers();
    let path = req.uri().path();
    info!("handle request: {}", path);

    // Echo HTTP headers
    headers.iter().for_each(|(name, value)| {
        echo_headers.insert(name, value.clone());
    });

    // Delay response if path can be interpreted as number of seconds
    if path.len() > 1 {
        if let Ok(delay) = path[1..].parse::<u64>() {
            debug!("delaying response for {}s", delay);
            tokio::time::sleep(Duration::from_secs(delay)).await;
        }
    }

    // Echo whole body
    let (_req_parts, req_body) = req.into_parts();
    let body_bytes = req_body
        .collect()
        .await
        .expect("Looks like a BUG!")
        .to_bytes();
    *response.body_mut() = Full::from(body_bytes);

    Ok(response)
}
