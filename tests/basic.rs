mod common;

use crate::common::run_test_with_config;
use common::{init_logging, test_one_case, TestConfig};
use futures_util::future::join_all;

const TEST_CONFIG_PATH: &str = "tests/configs/integration/basic.yaml";
const TEST_PORT: u16 = 3000;

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

#[tokio::test]
async fn basic_functionality() {
    init_logging();

    let result = run_test_with_config(TEST_CONFIG_PATH, TEST_PORT, 60, false, async {
        let client = reqwest::Client::new();
        let tasks: Vec<_> = prepare_test_cases()
            .into_iter()
            .map(|t| test_one_case(&client, t))
            .collect();
        join_all(tasks).await;
    })
    .await;

    assert_eq!(result, Ok(()))
}
