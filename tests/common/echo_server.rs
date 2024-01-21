use hyper::service::{make_service_fn, service_fn};
use hyper::{Body, Error, Request, Response, Server};
use std::convert::Infallible;
use std::net::SocketAddr;
use std::time::Duration;
use tracing::{debug, info};

pub async fn echo_server(port: u16) -> Result<(), Error> {
    info!("create echo server on port: {}", port);
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let server = Server::bind(&address).serve(make_service_fn(|_server| async {
        Ok::<_, Infallible>(service_fn(handle_request))
    }));

    // Allow server to be killed.
    let server = server.with_graceful_shutdown(async {
        tokio::signal::ctrl_c()
            .await
            .expect("Failed to add signal handler")
    });

    tokio::task::spawn(server).await.unwrap()
}

async fn handle_request(req: Request<Body>) -> Result<Response<Body>, Infallible> {
    let mut response = Response::new(Body::empty());
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
    *response.body_mut() = req.into_body();

    Ok(response)
}
