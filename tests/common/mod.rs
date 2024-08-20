pub mod echo_server;

use echo_server::{echo_server, tls_echo_server};
use futures_util::Future;
use http_dragonfly::{cli::CliConfig, context::RootOsEnvironment};
use hyper::header::HeaderValue;
use reqwest::Client;
use std::{env, io, sync::LazyLock, time::Duration};

const SERVER_CERT_BUNDLE: &str = "/end.crt";
const SERVER_PRIVATE_KEY: &str = "/test-server.key";

pub struct TestConfig {
    pub description: &'static str,
    pub port: u16,
    pub include_wrong_port: bool,
    pub include_timeout_target: bool,
    pub include_good_target: bool,
    pub expected_status: u16,
    pub expected_x_target_id_header: Option<&'static str>,
}

impl Default for TestConfig {
    fn default() -> Self {
        Self {
            description: "UNDEFINED",
            port: 0,
            include_wrong_port: false,
            include_timeout_target: false,
            include_good_target: true,
            expected_status: 200,
            expected_x_target_id_header: Some("GOOD"),
        }
    }
}

pub fn init_logging() -> bool {
    static INITIALIZED: LazyLock<bool> = LazyLock::new(|| {
        if env::var("RUST_LOG").is_ok() {
            tracing_subscriber::fmt::init();
        };
        true
    });

    *INITIALIZED
}

pub async fn run_test_with_config(
    config_path: &str,
    echo_port: u16,
    timeout_sec: u64,
    is_tls: bool,
    test: impl Future,
) -> Result<(), String> {
    if is_tls {
        let out_dir = env::var("OUT_DIR").unwrap();
        let cert_path = format!("{out_dir}/{SERVER_CERT_BUNDLE}");
        let key_path = format!("{out_dir}/{SERVER_PRIVATE_KEY}");

        run_test_with_config_and_server(
            config_path,
            timeout_sec,
            tls_echo_server(echo_port, cert_path, key_path),
            test,
        )
        .await
    } else {
        run_test_with_config_and_server(config_path, timeout_sec, echo_server(echo_port), test)
            .await
    }
}

async fn run_test_with_config_and_server(
    config_path: &str,
    timeout_sec: u64,
    echo_server: impl Future<Output = Result<(), io::Error>>,
    test: impl Future,
) -> Result<(), String> {
    let cli_config = CliConfig::from_config_path(config_path.into());
    let env_provider = RootOsEnvironment::new("^TEST_HTTP_ENV_[A-Z0-9]+$");
    let server = http_dragonfly::run(cli_config, env_provider);
    let timer = tokio::time::sleep(Duration::from_secs(timeout_sec));

    tokio::select! {
        _ = server => Err("http-dragonfly server has been crashed".into()),
        _ = echo_server => Err("echo server has been crashed".into()),
        _ = timer => Err("test has been timed out".into()),
        _result = test => Ok(())
    }
}

pub async fn test_one_case(client: &Client, test_config: TestConfig) {
    let mut req = client.get(format!("http://localhost:{}/", test_config.port));
    if test_config.include_wrong_port {
        req = req.header("x-include-wrong-port", "yes")
    }
    if test_config.include_timeout_target {
        req = req.header("x-include-timeout", "yes")
    }
    if test_config.include_good_target {
        req = req.header("x-include-good", "yes")
    }

    let resp = req.send().await.unwrap();
    assert_eq!(
        resp.status().as_u16(),
        test_config.expected_status,
        "{}: request to port {}, `{}` status expected, with: wrong_port={}, timeout_target={}, good_target={}",
        test_config.description,
        test_config.port,
        test_config.expected_status,
        test_config.include_wrong_port,
        test_config.include_timeout_target,
        test_config.include_good_target
    );

    let target_id_header = test_config
        .expected_x_target_id_header
        .map(|target_id| HeaderValue::from_str(target_id).unwrap());

    assert_eq!(
        resp.headers().get("x-target-id"),
        target_id_header.as_ref(),
        "{}: request to port {}, expected X-Target-Id header `{:?}`",
        test_config.description,
        test_config.port,
        test_config.expected_x_target_id_header
    );
}
