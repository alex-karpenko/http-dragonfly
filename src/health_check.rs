use hyper::{
    server::conn::AddrStream,
    service::{make_service_fn, service_fn},
    Body, Response, Server,
};
use std::{
    convert::Infallible,
    net::{Ipv4Addr, SocketAddr},
    time::Duration,
};
use tracing::{debug, info};

use crate::{shutdown_signal, PinnedBoxedServerFuture};

/// Health check handler
///
/// Listens to health-check port and responds 200 OK to any GET request
async fn handle(addr: SocketAddr) -> Result<Response<Body>, hyper::Error> {
    debug!("health check, from={}", addr);
    Ok(Response::new(Body::from("OK\n")))
}

/// Health check handler builder
pub fn new(port: u16, timeout_sec: u64) -> PinnedBoxedServerFuture {
    info!("Creating health check handler on *:{}", port);

    let ip = Ipv4Addr::new(0, 0, 0, 0);
    let socket = SocketAddr::new(ip.into(), port);
    let server = Server::bind(&socket);
    let server = server.http1_header_read_timeout(Duration::from_secs(timeout_sec));

    let make_service = make_service_fn(move |conn: &AddrStream| {
        let addr = conn.remote_addr();
        let service = service_fn(move |_req| handle(addr));

        async move { Ok::<_, Infallible>(service) }
    });

    let server = server
        .serve(make_service)
        .with_graceful_shutdown(shutdown_signal("Health check handler".into()));

    Box::pin(server)
}
