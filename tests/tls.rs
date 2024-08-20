mod common;

use crate::common::run_test_with_config;
use common::{init_logging, test_one_case, TestConfig};
use futures_util::future::join_all;

const TEST_CONFIG_PATH: &str = "tests/configs/integration/tls.yaml";
const TEST_PORT: u16 = 3001;

fn prepare_test_cases() -> Vec<TestConfig> {
    vec![
        TestConfig {
            description: "invalid cert with verification enabled",
            port: 9000,
            expected_status: 502,
            expected_x_target_id_header: None,
            ..TestConfig::default()
        },
        TestConfig {
            description: "invalid cert with verification disabled",
            port: 9001,
            ..TestConfig::default()
        },
        TestConfig {
            description: "valid listener cert with ca bundle",
            port: 9002,
            ..TestConfig::default()
        },
        TestConfig {
            description: "valid listener cert w/o ca bundle",
            port: 9003,
            expected_status: 502,
            ..TestConfig::default()
        },
        TestConfig {
            description: "invalid cert with target verification disabled",
            port: 9004,
            ..TestConfig::default()
        },
        TestConfig {
            description: "valid target cert bundle with absent listener cert",
            port: 9005,
            ..TestConfig::default()
        },
    ]
}

#[tokio::test]
async fn custom_tls_config() {
    init_logging();

    let result = run_test_with_config(TEST_CONFIG_PATH, TEST_PORT, 60, true, async {
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
