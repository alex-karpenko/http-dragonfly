mod common;

use crate::common::run_test_with_config;
use common::{init_logging, test_one_case, TestConfig};
use futures_util::future::join_all;
use std::{env, fs};

const TEST_CONFIG_TEMPLATE: &str = "tests/configs/integration/tls.yaml";
const TEST_PORT: u16 = 3001;

/// The template's `ca:` paths point at the literal string `tests/tls/`, which this
/// resolves to the current build's `OUT_DIR` and writes out as a real config file.
/// `OUT_DIR` is where `create-test-certificates.sh` actually writes the CA bundle
/// (see build.rs), and it's unique per build -- unlike a shared, mutable path, it
/// can never be raced by an unrelated, concurrent `cargo check`/`cargo build` (e.g.
/// an IDE background build) overwriting the certs this test depends on mid-run.
fn resolve_test_config_path() -> String {
    let out_dir =
        env::var("OUT_DIR").expect("OUT_DIR must be set by cargo for build-script crates");
    let template =
        fs::read_to_string(TEST_CONFIG_TEMPLATE).expect("failed to read TLS test config template");
    let resolved = template.replace("tests/tls/", &format!("{out_dir}/"));
    let resolved_path = format!("{out_dir}/tls-test-config.yaml");
    fs::write(&resolved_path, resolved).expect("failed to write resolved TLS test config");
    resolved_path
}

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

    let config_path = resolve_test_config_path();
    let result = run_test_with_config(&config_path, TEST_PORT, 60, true, async {
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
