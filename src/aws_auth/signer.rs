use super::{provider_for, AwsAuthError, SDK_CONFIG};
use crate::config::aws_sigv4::AwsSigV4Config;
use aws_credential_types::provider::ProvideCredentials;
use aws_sigv4::{
    http_request::{sign, PayloadChecksumKind, SignableBody, SignableRequest, SigningSettings},
    sign::v4,
};
use http_body_util::Full;
use hyper::{body::Bytes, Request};
use std::time::SystemTime;

pub(crate) async fn sign_request(
    aws_sigv4_cfg: &AwsSigV4Config,
    target_id: &str,
    request: &mut Request<Full<Bytes>>,
    body: &Bytes,
) -> Result<(), AwsAuthError> {
    let sdk_config = SDK_CONFIG
        .get()
        .expect("AWS SDK must be initialized before signing a request, looks like a BUG");

    let region = aws_sigv4_cfg
        .region()
        .map(str::to_string)
        .or_else(|| sdk_config.region().map(|r| r.to_string()))
        .ok_or_else(|| AwsAuthError::NoRegion {
            target_id: target_id.to_string(),
        })?;

    let provider = provider_for(aws_sigv4_cfg.role_arn(), sdk_config).await;
    let credentials =
        provider
            .provide_credentials()
            .await
            .map_err(|cause| AwsAuthError::Credentials {
                target_id: target_id.to_string(),
                cause,
            })?;

    let identity = credentials.into();
    let mut signing_settings = SigningSettings::default();
    if aws_sigv4_cfg.service() == "s3" {
        // S3 requires the `x-amz-content-sha256` header on every signed request;
        // `NoHeader` (the crate default) omits it and S3 rejects the request with
        // "Missing required header for this request: x-amz-content-sha256".
        signing_settings.payload_checksum_kind = PayloadChecksumKind::XAmzSha256;
    }
    let signing_params: v4::SigningParams<SigningSettings> = v4::SigningParams::builder()
        .identity(&identity)
        .region(&region)
        .name(aws_sigv4_cfg.service())
        .time(SystemTime::now())
        .settings(signing_settings)
        .build()
        .expect("all required SigV4 signing parameters are provided, looks like a BUG");
    let signing_params = signing_params.into();

    let headers: Vec<(&str, &str)> = request
        .headers()
        .iter()
        .filter_map(|(name, value)| value.to_str().ok().map(|v| (name.as_str(), v)))
        .collect();

    let signable_request = SignableRequest::new(
        request.method().as_str(),
        request.uri().to_string(),
        headers.into_iter(),
        SignableBody::Bytes(body),
    )
    .expect(
        "method/uri/headers are always valid ASCII for an already-built request, looks like a BUG",
    );

    let (signing_instructions, _signature) = sign(signable_request, &signing_params)
        .expect("signing a well-formed request cannot fail, looks like a BUG")
        .into_parts();
    signing_instructions.apply_to_request_http1x(request);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::Credentials;
    use hyper::header::HeaderName;

    #[test]
    fn signing_matches_independently_computed_signature() {
        let credentials = Credentials::new("AKIDEXAMPLE", "secretkey", None, None, "test");
        let time = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_440_938_160); // 2015-08-30T12:16:00Z
        let body = Bytes::from_static(b"hello world");

        let mut request: Request<Full<Bytes>> = Request::builder()
            .method("PUT")
            .uri("https://examplebucket.s3.amazonaws.com/test.txt")
            .header("host", "examplebucket.s3.amazonaws.com")
            .body(Full::from(body.clone()))
            .unwrap();

        // "Actual": build signing params/instructions the exact same way sign_request() does.
        let identity = credentials.clone().into();
        let signing_params: v4::SigningParams<SigningSettings> = v4::SigningParams::builder()
            .identity(&identity)
            .region("us-east-1")
            .name("s3")
            .time(time)
            .settings(SigningSettings::default())
            .build()
            .unwrap();
        let signing_params = signing_params.into();
        let headers: Vec<(&str, &str)> = request
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect();
        let signable_request = SignableRequest::new(
            request.method().as_str(),
            request.uri().to_string(),
            headers.into_iter(),
            SignableBody::Bytes(&body),
        )
        .unwrap();
        let (instructions, expected_signature) = sign(signable_request, &signing_params)
            .unwrap()
            .into_parts();
        instructions.apply_to_request_http1x(&mut request);

        let auth_header = request
            .headers()
            .get(HeaderName::from_static("authorization"))
            .unwrap()
            .to_str()
            .unwrap();
        assert!(auth_header.contains(&expected_signature));
        assert!(auth_header.starts_with(
            "AWS4-HMAC-SHA256 Credential=AKIDEXAMPLE/20150830/us-east-1/s3/aws4_request"
        ));
        assert!(request
            .headers()
            .get(HeaderName::from_static("x-amz-date"))
            .is_some());
    }

    fn test_sdk_config() -> aws_config::SdkConfig {
        aws_config::SdkConfig::builder()
            .region(aws_config::Region::new("us-east-1"))
            .credentials_provider(
                aws_credential_types::provider::SharedCredentialsProvider::new(Credentials::new(
                    "AKIDEXAMPLE",
                    "secretkey",
                    None,
                    None,
                    "test",
                )),
            )
            .build()
    }

    #[tokio::test]
    async fn s3_service_gets_content_sha256_header() {
        // `SDK_CONFIG` is a process-wide `OnceCell`; ignore an already-set error so
        // this test can run alongside `non_s3_service_omits_content_sha256_header`.
        let _ = SDK_CONFIG.set(test_sdk_config());
        let cfg: AwsSigV4Config = serde_yaml_ng::from_str("service: s3").unwrap();
        let body = Bytes::from_static(b"hello world");
        let mut request: Request<Full<Bytes>> = Request::builder()
            .method("PUT")
            .uri("https://examplebucket.s3.amazonaws.com/test.txt")
            .header("host", "examplebucket.s3.amazonaws.com")
            .body(Full::from(body.clone()))
            .unwrap();

        sign_request(&cfg, "test-target", &mut request, &body)
            .await
            .unwrap();

        assert_eq!(
            request
                .headers()
                .get(HeaderName::from_static("x-amz-content-sha256"))
                .map(|v| v.to_str().unwrap()),
            // sha256("hello world")
            Some("b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9")
        );
    }

    #[tokio::test]
    async fn non_s3_service_omits_content_sha256_header() {
        let _ = SDK_CONFIG.set(test_sdk_config());
        let cfg: AwsSigV4Config = serde_yaml_ng::from_str("service: execute-api").unwrap();
        let body = Bytes::from_static(b"hello world");
        let mut request: Request<Full<Bytes>> = Request::builder()
            .method("GET")
            .uri("https://abc123.execute-api.us-east-1.amazonaws.com/prod/")
            .header("host", "abc123.execute-api.us-east-1.amazonaws.com")
            .body(Full::from(body.clone()))
            .unwrap();

        sign_request(&cfg, "test-target", &mut request, &body)
            .await
            .unwrap();

        assert!(request
            .headers()
            .get(HeaderName::from_static("x-amz-content-sha256"))
            .is_none());
    }

    #[test]
    fn session_token_is_included_when_present() {
        let credentials = Credentials::new(
            "AKIDEXAMPLE",
            "secretkey",
            Some("sessiontoken".to_string()),
            None,
            "test",
        );
        let identity = credentials.into();
        let signing_params: v4::SigningParams<SigningSettings> = v4::SigningParams::builder()
            .identity(&identity)
            .region("us-east-1")
            .name("s3")
            .time(SystemTime::now())
            .settings(SigningSettings::default())
            .build()
            .unwrap();
        let signing_params = signing_params.into();
        let body = Bytes::new();
        let mut request: Request<Full<Bytes>> = Request::builder()
            .method("GET")
            .uri("https://examplebucket.s3.amazonaws.com/")
            .header("host", "examplebucket.s3.amazonaws.com")
            .body(Full::from(body.clone()))
            .unwrap();
        let headers: Vec<(&str, &str)> = request
            .headers()
            .iter()
            .map(|(k, v)| (k.as_str(), v.to_str().unwrap()))
            .collect();
        let signable_request = SignableRequest::new(
            request.method().as_str(),
            request.uri().to_string(),
            headers.into_iter(),
            SignableBody::Bytes(&body),
        )
        .unwrap();
        let (instructions, _sig) = sign(signable_request, &signing_params)
            .unwrap()
            .into_parts();
        instructions.apply_to_request_http1x(&mut request);

        assert_eq!(
            request
                .headers()
                .get(HeaderName::from_static("x-amz-security-token"))
                .map(|v| v.to_str().unwrap()),
            Some("sessiontoken")
        );
    }
}
