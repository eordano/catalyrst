use std::time::Duration;

pub const USER_AGENT: &str = concat!("catalyrst/", env!("CARGO_PKG_VERSION"));

/// Shape of a catalyrst outbound HTTP client. Redirect following is opt-in: for
/// user-supplied URLs each hop has to be re-checked against the SSRF guards, which
/// reqwest's own redirect policy cannot do.
#[derive(Clone, Debug)]
pub struct HttpClientCfg {
    pub total_timeout: Duration,
    pub connect_timeout: Duration,
    pub pool_idle_timeout: Duration,
    pub pool_max_idle_per_host: usize,
    pub max_redirects: Option<usize>,
    pub user_agent: Option<String>,
}

impl Default for HttpClientCfg {
    fn default() -> Self {
        Self {
            total_timeout: Duration::from_secs(30),
            connect_timeout: Duration::from_secs(10),
            pool_idle_timeout: Duration::from_secs(90),
            pool_max_idle_per_host: 16,
            max_redirects: None,
            user_agent: None,
        }
    }
}

impl HttpClientCfg {
    pub fn with_total_timeout(mut self, timeout: Duration) -> Self {
        self.total_timeout = timeout;
        self
    }

    pub fn with_connect_timeout(mut self, timeout: Duration) -> Self {
        self.connect_timeout = timeout;
        self
    }

    pub fn with_pool(mut self, idle_timeout: Duration, max_idle_per_host: usize) -> Self {
        self.pool_idle_timeout = idle_timeout;
        self.pool_max_idle_per_host = max_idle_per_host;
        self
    }

    pub fn following_redirects(mut self, max: usize) -> Self {
        self.max_redirects = Some(max);
        self
    }

    pub fn with_user_agent(mut self, user_agent: impl Into<String>) -> Self {
        self.user_agent = Some(user_agent.into());
        self
    }

    /// The configured builder, for callers that need extra knobs -- DNS pinning via
    /// `resolve`, custom headers, proxies.
    pub fn builder(&self) -> reqwest::ClientBuilder {
        let redirect = match self.max_redirects {
            Some(max) => reqwest::redirect::Policy::limited(max),
            None => reqwest::redirect::Policy::none(),
        };
        reqwest::Client::builder()
            .user_agent(self.user_agent.as_deref().unwrap_or(USER_AGENT))
            .timeout(self.total_timeout)
            .connect_timeout(self.connect_timeout)
            .pool_idle_timeout(self.pool_idle_timeout)
            .pool_max_idle_per_host(self.pool_max_idle_per_host)
            .redirect(redirect)
    }
}

pub fn try_http_client(cfg: &HttpClientCfg) -> reqwest::Result<reqwest::Client> {
    cfg.builder().build()
}

/// Builds the client, or falls back to a default one so a TLS-backend hiccup degrades
/// the timeouts rather than taking the service down at startup.
pub fn http_client(name: &str, cfg: &HttpClientCfg) -> reqwest::Client {
    match try_http_client(cfg) {
        Ok(client) => client,
        Err(err) => {
            tracing::warn!(client = name, %err, "http client build failed; using defaults");
            reqwest::Client::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn user_agent_is_the_crate_version() {
        assert_eq!(USER_AGENT, format!("catalyrst/{}", crate::VERSION));
    }

    #[test]
    fn defaults_do_not_follow_redirects() {
        assert!(HttpClientCfg::default().max_redirects.is_none());
    }

    #[test]
    fn builders_produce_a_client() {
        assert!(try_http_client(&HttpClientCfg::default()).is_ok());
        let cfg = HttpClientCfg::default()
            .with_total_timeout(Duration::from_secs(5))
            .with_connect_timeout(Duration::from_secs(2))
            .with_pool(Duration::from_secs(30), 4)
            .following_redirects(3)
            .with_user_agent("catalyrst-test/1");
        assert_eq!(cfg.max_redirects, Some(3));
        assert!(try_http_client(&cfg).is_ok());
        let _ = http_client("test", &cfg);
    }
}
