mod common;

use crate::common::run_test_with_config;
use insta::assert_debug_snapshot;

const TEST_CONFIG_PATH: &str = "tests/configs/integration/basic.yaml";

#[tokio::test]
async fn basic_functionality() {
    let result = run_test_with_config(TEST_CONFIG_PATH, 3000, 60, async {
        let client = reqwest::Client::new();
        // The simplest request
        assert_eq!(
            client
                .get("http://localhost:8000/")
                .send()
                .await
                .unwrap()
                .status(),
            200,
            "simple request to 8000, 200 expected"
        );
        // Request with timeout
        assert_eq!(
            client
                .get("http://localhost:8000/")
                .header("X-Delay", "2")
                .send()
                .await
                .unwrap()
                .status(),
            504,
            "request to 8000 with timeout, 504 is expected"
        );
        assert_debug_snapshot!(
            client
                .get("http://localhost:8000/5")
                .send()
                .await
                .unwrap()
                .text()
                .await
        );

        // failed_then_ok
        let wrong = client.get("http://localhost:8001/").send().await.unwrap();
        let good = client
            .get("http://localhost:8001/")
            .header("x-skip-wrong", "yes")
            .send()
            .await
            .unwrap();
        let timeout = client
            .get("http://localhost:8001/")
            .header("x-skip-wrong", "yes")
            .header("x-include-timeout", "yes")
            .send()
            .await
            .unwrap();
        assert_eq!(
            wrong.status(),
            502,
            "request to invalid target, 502 expected"
        );
        assert_eq!(good.status(), 200, "request to valid target, 200 expected");
        assert_eq!(
            timeout.status(),
            504,
            "request to delayed target, 504 expected"
        );
        assert_debug_snapshot!(wrong.headers().get("x-target-id"));
        assert_debug_snapshot!(good.headers().get("x-target-id"));
        assert_debug_snapshot!(timeout.headers().get("x-target-id"));

        // failed_then_target_id
        let wrong = client.get("http://localhost:8002/").send().await.unwrap();
        let good = client
            .get("http://localhost:8002/")
            .header("x-skip-wrong", "yes")
            .send()
            .await
            .unwrap();
        let timeout = client
            .get("http://localhost:8002/")
            .header("x-skip-wrong", "yes")
            .header("x-include-timeout", "yes")
            .send()
            .await
            .unwrap();
        assert_eq!(
            wrong.status(),
            502,
            "request to invalid target, 502 expected"
        );
        assert_eq!(good.status(), 200, "request to valid target, 200 expected");
        assert_eq!(
            timeout.status(),
            504,
            "request to delayed target, 504 expected"
        );
        assert_debug_snapshot!(wrong.headers().get("x-target-id"));
        assert_debug_snapshot!(good.headers().get("x-target-id"));
        assert_debug_snapshot!(timeout.headers().get("x-target-id"));

        // failed_then_override
        let wrong = client.get("http://localhost:8003/").send().await.unwrap();
        let good = client
            .get("http://localhost:8003/")
            .header("x-skip-wrong", "yes")
            .send()
            .await
            .unwrap();
        let timeout = client
            .get("http://localhost:8003/")
            .header("x-skip-wrong", "yes")
            .header("x-include-timeout", "yes")
            .send()
            .await
            .unwrap();
        assert_eq!(
            wrong.status(),
            502,
            "request to invalid target, 502 expected"
        );
        assert_eq!(good.status(), 200, "request to valid target, 200 expected");
        assert_eq!(
            timeout.status(),
            504,
            "request to delayed target, 504 expected"
        );
        assert_debug_snapshot!(wrong.headers().get("x-target-id"));
        assert_debug_snapshot!(good.headers().get("x-target-id"));
        assert_debug_snapshot!(timeout.headers().get("x-target-id"));

    })
    .await;

    assert_eq!(result, Ok(()))
}
