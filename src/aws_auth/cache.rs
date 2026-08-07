use aws_credential_types::{
    provider::{self, future, ProvideCredentials},
    Credentials,
};
use std::{
    fmt,
    sync::Arc,
    time::{Duration, SystemTime},
};
use tokio::sync::RwLock;
use tracing::{debug, warn};

#[derive(Debug)]
struct CachedEntry {
    credentials: Credentials,
    refresh_at: Option<SystemTime>,
    expires_at: Option<SystemTime>,
}

/// Wraps an inner [`ProvideCredentials`] and caches its result, refreshing
/// proactively at half of the credentials' remaining validity window
/// instead of waiting until they're about to expire.
pub(crate) struct RefreshingCredentials {
    inner: Arc<dyn ProvideCredentials>,
    cache: RwLock<Option<CachedEntry>>,
    label: String,
}

impl fmt::Debug for RefreshingCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RefreshingCredentials")
            .field("label", &self.label)
            .finish()
    }
}

impl RefreshingCredentials {
    pub(crate) fn new(inner: Arc<dyn ProvideCredentials>, label: impl Into<String>) -> Self {
        Self {
            inner,
            cache: RwLock::new(None),
            label: label.into(),
        }
    }

    async fn get_or_refresh(&self) -> provider::Result {
        let now = SystemTime::now();
        {
            let cache = self.cache.read().await;
            if let Some(entry) = cache.as_ref() {
                if entry.refresh_at.is_none_or(|refresh_at| now < refresh_at) {
                    return Ok(entry.credentials.clone());
                }
            }
        }

        let mut cache = self.cache.write().await;
        // Double-check: another task may have refreshed while we waited for the write lock.
        let now = SystemTime::now();
        if let Some(entry) = cache.as_ref() {
            if entry.refresh_at.is_none_or(|refresh_at| now < refresh_at) {
                return Ok(entry.credentials.clone());
            }
        }

        match self.inner.provide_credentials().await {
            Ok(credentials) => {
                let now = SystemTime::now();
                let expires_at = credentials.expiry();
                let refresh_at = expires_at.map(|expires_at| {
                    let validity = expires_at.duration_since(now).unwrap_or(Duration::ZERO);
                    now + validity / 2
                });
                debug!(label = %self.label, ?expires_at, ?refresh_at, "aws credentials refreshed");
                *cache = Some(CachedEntry {
                    credentials: credentials.clone(),
                    refresh_at,
                    expires_at,
                });
                Ok(credentials)
            }
            Err(err) => {
                if let Some(entry) = cache.as_ref() {
                    if entry.expires_at.is_none_or(|expires_at| now < expires_at) {
                        warn!(
                            label = %self.label,
                            error = %err,
                            "failed to refresh aws credentials, serving stale cached credentials"
                        );
                        return Ok(entry.credentials.clone());
                    }
                }
                Err(err)
            }
        }
    }
}

impl ProvideCredentials for RefreshingCredentials {
    fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
    where
        Self: 'a,
    {
        future::ProvideCredentials::new(self.get_or_refresh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use aws_credential_types::provider::error::CredentialsError;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tokio::sync::Mutex as AsyncMutex;

    #[derive(Debug, Clone)]
    enum FakeResponse {
        Creds { validity: Option<Duration> },
        Err,
    }

    #[derive(Debug)]
    struct FakeProvider {
        calls: AtomicUsize,
        script: AsyncMutex<Vec<FakeResponse>>,
    }

    impl FakeProvider {
        fn new(script: Vec<FakeResponse>) -> Self {
            Self {
                calls: AtomicUsize::new(0),
                script: AsyncMutex::new(script),
            }
        }

        fn call_count(&self) -> usize {
            self.calls.load(Ordering::SeqCst)
        }

        async fn next(&self) -> provider::Result {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let mut script = self.script.lock().await;
            let response = if script.len() > 1 {
                script.remove(0)
            } else {
                script[0].clone()
            };
            match response {
                FakeResponse::Creds { validity } => Ok(Credentials::new(
                    "AKIDFAKE",
                    "fake-secret",
                    None,
                    validity.map(|v| SystemTime::now() + v),
                    "fake-provider",
                )),
                FakeResponse::Err => Err(CredentialsError::provider_error("fake failure")),
            }
        }
    }

    impl ProvideCredentials for FakeProvider {
        fn provide_credentials<'a>(&'a self) -> future::ProvideCredentials<'a>
        where
            Self: 'a,
        {
            future::ProvideCredentials::new(self.next())
        }
    }

    #[tokio::test]
    async fn caches_until_half_life_then_refreshes() {
        let fake = Arc::new(FakeProvider::new(vec![FakeResponse::Creds {
            validity: Some(Duration::from_millis(200)),
        }]));
        let cache = RefreshingCredentials::new(fake.clone(), "test");

        cache.provide_credentials().await.unwrap();
        assert_eq!(fake.call_count(), 1);

        // well before half-life (100ms): still cached
        tokio::time::sleep(Duration::from_millis(40)).await;
        cache.provide_credentials().await.unwrap();
        assert_eq!(
            fake.call_count(),
            1,
            "should still be serving cached credentials before half-life"
        );

        // past half-life: refreshes
        tokio::time::sleep(Duration::from_millis(80)).await;
        cache.provide_credentials().await.unwrap();
        assert_eq!(fake.call_count(), 2, "should refresh once past half-life");
    }

    #[tokio::test]
    async fn no_expiry_is_cached_forever() {
        let fake = Arc::new(FakeProvider::new(vec![FakeResponse::Creds {
            validity: None,
        }]));
        let cache = RefreshingCredentials::new(fake.clone(), "test");

        for _ in 0..5 {
            cache.provide_credentials().await.unwrap();
        }
        assert_eq!(fake.call_count(), 1);
    }

    #[tokio::test]
    async fn concurrent_refreshes_single_flight() {
        let fake = Arc::new(FakeProvider::new(vec![FakeResponse::Creds {
            validity: Some(Duration::from_millis(1)),
        }]));
        let cache = Arc::new(RefreshingCredentials::new(fake.clone(), "test"));

        cache.provide_credentials().await.unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await; // past half-life

        let tasks: Vec<_> = (0..20)
            .map(|_| {
                let cache = cache.clone();
                tokio::spawn(async move { cache.provide_credentials().await.unwrap() })
            })
            .collect();
        for t in tasks {
            t.await.unwrap();
        }

        assert_eq!(
            fake.call_count(),
            2,
            "20 concurrent refreshers should collapse into a single real fetch"
        );
    }

    #[tokio::test]
    async fn serves_stale_credentials_when_refresh_fails() {
        let fake = Arc::new(FakeProvider::new(vec![
            FakeResponse::Creds {
                validity: Some(Duration::from_millis(100)),
            },
            FakeResponse::Err,
        ]));
        let cache = RefreshingCredentials::new(fake.clone(), "test");

        let first = cache.provide_credentials().await.unwrap();
        // past half-life (50ms) but before real expiry (100ms)
        tokio::time::sleep(Duration::from_millis(70)).await;
        let second = cache.provide_credentials().await.unwrap();

        assert_eq!(first.access_key_id(), second.access_key_id());
        assert_eq!(
            fake.call_count(),
            2,
            "a refresh attempt should have been made and failed"
        );
    }

    #[tokio::test]
    async fn errors_once_stale_credentials_truly_expire() {
        let fake = Arc::new(FakeProvider::new(vec![
            FakeResponse::Creds {
                validity: Some(Duration::from_millis(30)),
            },
            FakeResponse::Err,
        ]));
        let cache = RefreshingCredentials::new(fake.clone(), "test");

        cache.provide_credentials().await.unwrap();
        tokio::time::sleep(Duration::from_millis(60)).await; // past real expiry too
        let result = cache.provide_credentials().await;

        assert!(result.is_err());
    }
}
