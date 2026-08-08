use http_dragonfly::{cli::CliConfig, context::RootOsEnvironment};
use reqwest::Client;
use std::time::Duration;
use testcontainers_modules::{
    localstack::LocalStack,
    testcontainers::{runners::AsyncRunner, ImageExt},
};

const TEST_CONFIG_PATH: &str = "tests/configs/integration/aws-sigv4.yaml";
const BUCKET: &str = "http-dragonfly-test-bucket";

#[tokio::test]
async fn signs_requests_to_localstack_s3() {
    let container = LocalStack::default()
        .with_env_var("SERVICES", "s3,sts,iam")
        .start()
        .await
        .expect("failed to start localstack container, is Docker running?");
    let ls_port = container
        .get_host_port_ipv4(4566)
        .await
        .expect("failed to get localstack mapped port");

    // SAFETY: this test file has a single test function, so there's no other task
    // in this process racing these env var writes.
    unsafe {
        std::env::set_var("AWS_ACCESS_KEY_ID", "test");
        std::env::set_var("AWS_SECRET_ACCESS_KEY", "test");
        std::env::set_var("AWS_REGION", "us-east-1");
        std::env::set_var("AWS_ENDPOINT_URL", format!("http://127.0.0.1:{ls_port}"));
        std::env::set_var("TEST_HTTP_ENV_LOCALSTACK_PORT", ls_port.to_string());
    }

    let cli_config = CliConfig::from_config_path(TEST_CONFIG_PATH.into());
    let env_provider = RootOsEnvironment::new("^TEST_HTTP_ENV_[A-Z0-9_]+$");
    let server = http_dragonfly::run(cli_config, env_provider);
    let timer = tokio::time::sleep(Duration::from_secs(120));

    let test = async {
        let client = Client::new();

        // `run()` does async AWS SDK init (and, for the assumed-role listener, a real
        // STS AssumeRole call) before it binds any listener, so wait for each port to
        // actually accept connections instead of assuming it's instantly up.
        for port in [9101, 9102] {
            wait_for_listener(port, Duration::from_secs(60)).await;
        }

        // Create the bucket via the default-credentials-signed listener.
        let resp = client
            .put(format!("http://localhost:9101/{BUCKET}/"))
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "create bucket failed: {}",
            resp.status()
        );

        // PUT + GET an object signed with default (env) credentials.
        let resp = client
            .put(format!("http://localhost:9101/{BUCKET}/hello.txt"))
            .body("hello from default creds")
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "put object failed: {}",
            resp.status()
        );

        let resp = client
            .get(format!("http://localhost:9101/{BUCKET}/hello.txt"))
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "get object failed: {}",
            resp.status()
        );
        assert_eq!(resp.text().await.unwrap(), "hello from default creds");

        // PUT + GET an object signed via an assumed role.
        let resp = client
            .put(format!("http://localhost:9102/{BUCKET}/hello-role.txt"))
            .body("hello via assumed role")
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "put object via assumed role failed: {}",
            resp.status()
        );

        let resp = client
            .get(format!("http://localhost:9102/{BUCKET}/hello-role.txt"))
            .send()
            .await
            .unwrap();
        assert!(
            resp.status().is_success(),
            "get object via assumed role failed: {}",
            resp.status()
        );
        assert_eq!(resp.text().await.unwrap(), "hello via assumed role");
    };

    tokio::select! {
        _ = server => panic!("http-dragonfly server crashed"),
        _ = timer => panic!("test timed out"),
        _ = test => {}
    }
}

async fn wait_for_listener(port: u16, deadline: Duration) {
    let start = tokio::time::Instant::now();
    loop {
        if tokio::net::TcpStream::connect(("127.0.0.1", port))
            .await
            .is_ok()
        {
            return;
        }
        if start.elapsed() > deadline {
            panic!("listener on port {port} did not become ready within {deadline:?}");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
