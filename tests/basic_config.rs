mod common;

use std::env;

use crate::common::run_test_with_config;
use futures_util::future::join_all;
use reqwest::{header::HeaderValue, Client};

const TEST_CONFIG_PATH: &str = "tests/configs/integration/basic.yaml";

struct TestConfig {
    description: &'static str,
    port: u16,
    include_wrong_port: bool,
    include_timeout_target: bool,
    include_good_target: bool,
    expected_status: u16,
    expected_x_target_id_header: Option<&'static str>,
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

fn prepare_test_cases() -> Vec<TestConfig> {
    vec![
        TestConfig {
            description: "failed_then_ok",
            port: 8001,
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_ok",
            port: 8001,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("WRONG"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_ok",
            port: 8001,
            include_timeout_target: true,
            expected_status: 504,
            expected_x_target_id_header: Some("TIMEOUT"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_target_id",
            port: 8002,
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_target_id",
            port: 8002,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("WRONG"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_target_id",
            port: 8002,
            include_timeout_target: true,
            expected_status: 504,
            expected_x_target_id_header: Some("TIMEOUT"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_override",
            port: 8003,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_override",
            port: 8003,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("WRONG"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "failed_then_override",
            port: 8003,
            include_timeout_target: true,
            expected_status: 504,
            expected_x_target_id_header: Some("TIMEOUT"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_failed",
            port: 8004,
            include_timeout_target: true,
            include_wrong_port: true,
            expected_x_target_id_header: Some("GOOD"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_failed",
            port: 8004,
            include_good_target: false,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("WRONG"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_failed",
            port: 8004,
            include_good_target: false,
            include_timeout_target: true,
            expected_status: 504,
            expected_x_target_id_header: Some("TIMEOUT"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_target_id",
            port: 8005,
            include_timeout_target: true,
            include_wrong_port: true,
            expected_x_target_id_header: Some("GOOD"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_target_id",
            port: 8005,
            include_good_target: false,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("WRONG"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_target_id",
            port: 8005,
            include_good_target: false,
            include_timeout_target: true,
            expected_status: 500,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_override",
            port: 8006,
            include_timeout_target: true,
            include_wrong_port: true,
            expected_x_target_id_header: Some("GOOD"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_override",
            port: 8006,
            include_good_target: false,
            include_wrong_port: true,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "ok_then_override",
            port: 8006,
            include_good_target: false,
            include_timeout_target: true,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "always_target_id",
            port: 8007,
            include_timeout_target: true,
            include_wrong_port: true,
            expected_x_target_id_header: Some("GOOD"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "always_target_id",
            port: 8007,
            include_good_target: false,
            include_wrong_port: true,
            expected_status: 500,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "always_target_id",
            port: 8007,
            include_good_target: false,
            include_timeout_target: true,
            expected_status: 500,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "always_override",
            port: 8008,
            include_timeout_target: true,
            include_wrong_port: true,
            expected_status: 222,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "conditionals_routing",
            port: 8009,
            expected_x_target_id_header: Some("1"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "conditionals_routing",
            port: 8009,
            include_good_target: false,
            expected_x_target_id_header: Some("default"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "conditionals_routing",
            port: 8009,
            include_timeout_target: true,
            expected_status: 500,
            expected_x_target_id_header: Some("${CTX_TARGET_ID}"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "conditionals_routing",
            port: 8009,
            include_good_target: false,
            include_wrong_port: true,
            expected_status: 502,
            expected_x_target_id_header: Some("2"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "target_status_override",
            port: 8010,
            include_timeout_target: true,
            expected_status: 555,
            expected_x_target_id_header: Some("timeout_with_555_status"),
            ..TestConfig::default()
        },
        TestConfig {
            description: "target_status_override",
            port: 8010,
            include_wrong_port: true,
            expected_status: 500,
            expected_x_target_id_header: Some("wrong_port_with_dropped_status"),
            ..TestConfig::default()
        },
    ]
}

async fn test_one_strategy(client: &Client, test_config: TestConfig) {
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

#[tokio::test]
async fn basic_functionality() {
    if env::var("RUST_LOG").is_ok() {
        tracing_subscriber::fmt::init();
    }

    let result = run_test_with_config(TEST_CONFIG_PATH, 3000, 60, async {
        let client = reqwest::Client::new();
        let tasks: Vec<_> = prepare_test_cases()
            .into_iter()
            .map(|t| test_one_strategy(&client, t))
            .collect();
        join_all(tasks).await;
    })
    .await;

    assert_eq!(result, Ok(()))
}
