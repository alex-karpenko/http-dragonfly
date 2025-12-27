use http_body_util::{BodyExt, Full};
use http_dragonfly::signal::SignalHandler;
use hyper::{
    body::{Bytes, Incoming},
    service::service_fn,
    Request, Response,
};
use hyper_util::{
    rt::{TokioExecutor, TokioIo},
    server::conn::auto::Builder,
};
use rustls::{
    pki_types::{CertificateDer, PrivateKeyDer},
    ServerConfig,
};
use rustls_pki_types::pem::PemObject;
use std::{
    convert::Infallible,
    fs, io,
    net::{Ipv4Addr, SocketAddr},
    sync::Arc,
    time::Duration,
};
use tokio::{net::TcpListener, select, task::JoinSet};
use tokio_rustls::TlsAcceptor;
use tracing::{debug, error, info, warn};

pub async fn echo_server(port: u16) -> Result<(), anyhow::Error> {
    info!("create echo server on port: {}", port);

    let ip = Ipv4Addr::new(0, 0, 0, 0);
    let socket = SocketAddr::new(ip.into(), port);
    let listener = TcpListener::bind(&socket)
        .await
        .expect("unable to create echo server listener");
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

    tokio::task::spawn(server).await?
}

pub async fn tls_echo_server(
    port: u16,
    cert: impl Into<String>,
    key: impl Into<String>,
) -> Result<(), anyhow::Error> {
    info!("create tls echo server on port: {}", port);

    // Set a process wide default crypto provider.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let ip = Ipv4Addr::new(0, 0, 0, 0);
    let socket = SocketAddr::new(ip.into(), port);

    // Load public certificate.
    let certs = load_certs(&cert.into())?;
    // Load private key.
    let key = load_private_key(&key.into())?;

    let listener = TcpListener::bind(&socket)
        .await
        .expect("unable to create echo server listener");

    // Build TLS configuration.
    let mut server_config = ServerConfig::builder()
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|e| error(e.to_string()))?;
    server_config.alpn_protocols = vec![b"http/1.1".to_vec(), b"http/1.0".to_vec()];
    let tls_acceptor = TlsAcceptor::from(Arc::new(server_config));

    let mut signal_handler = SignalHandler::new("health");
    let mut join_set = JoinSet::new();
    let service = service_fn(handle_request);

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

                    let tls_acceptor = tls_acceptor.clone();
                    let serve_connection = async move {
                        let tls_stream = match tls_acceptor.accept(stream).await {
                            Ok(tls_stream) => tls_stream,
                            Err(err) => {
                                error!(error = ?err, address = %addr, "failed to perform tls handshake");
                                return;
                            }
                        };
                        if let Err(err) = Builder::new(TokioExecutor::new())
                            .serve_connection(TokioIo::new(tls_stream), service)
                            .await
                        {
                            error!(error = ?err, address = %addr, "failed to serve tls connection");
                        }
                    };

                    join_set.spawn(serve_connection);
                }
            }
        }
        Ok(())
    };

    tokio::task::spawn(server).await?
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

// Load public certificate from file.
fn load_certs(filename: &str) -> io::Result<Vec<CertificateDer<'static>>> {
    // Open certificate file.
    let certfile =
        fs::File::open(filename).map_err(|e| error(format!("failed to open {filename}: {e}")))?;
    let mut reader = io::BufReader::new(certfile);

    // Load and return certificate.
    CertificateDer::pem_reader_iter(&mut reader)
        .map(|cert| cert.map_err(|e| error(format!("unable to load PEN certificates: {e}"))))
        .collect()
}

// Load private key from file.
fn load_private_key(filename: &str) -> io::Result<PrivateKeyDer<'static>> {
    // Open keyfile.
    let keyfile =
        fs::File::open(filename).map_err(|e| error(format!("failed to open {filename}: {e}")))?;
    let mut reader = io::BufReader::new(keyfile);

    // Load and return a single private key.
    PrivateKeyDer::from_pem_reader(&mut reader)
        .map_err(|e| error(format!("failed to load private key: {e}")))
}

fn error(err: String) -> io::Error {
    io::Error::other(err)
}
