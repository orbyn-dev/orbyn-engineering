//! Whether the app's outside dependencies answer from here: GitHub, and each
//! provider's service. Asked for, never scheduled. A check sends one empty `GET` to
//! the host's front door and reads only the status line, so no credential, project
//! or thread detail leaves the machine.

use std::time::{Duration, Instant};

use orbyn_contracts::{DependencyCheck, DependencyHealth, ProviderHealth};

use crate::app::Core;
use crate::error::Result;
use crate::util::now;

const CHECK_TIMEOUT: Duration = Duration::from_secs(5);
const GITHUB: &str = "https://api.github.com/";

/// The front door of a provider family's service.
fn provider_service(family: &str) -> Option<&'static str> {
    match family {
        "claude" => Some("https://api.anthropic.com/"),
        "codex" => Some("https://api.openai.com/"),
        _ => None,
    }
}

fn host_of(url: &str) -> String {
    reqwest::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_owned))
        .unwrap_or_else(|| url.to_owned())
}

/// One `GET` to `url`. Any HTTP answer, even an error status, means the host is reachable.
pub(crate) async fn check_url(client: &reqwest::Client, url: &str) -> DependencyCheck {
    let started = Instant::now();
    let result = client.get(url).timeout(CHECK_TIMEOUT).send().await;
    let latency = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
    match result {
        Ok(response) => DependencyCheck {
            host: host_of(url),
            checked: true,
            reachable: true,
            status: Some(response.status().as_u16()),
            latency_ms: Some(latency),
            message: None,
        },
        Err(error) => DependencyCheck {
            host: host_of(url),
            checked: true,
            reachable: false,
            status: None,
            latency_ms: None,
            message: Some(if error.is_timeout() {
                "No answer in time.".to_owned()
            } else {
                error.without_url().to_string()
            }),
        },
    }
}

fn not_checked(url: &str) -> DependencyCheck {
    DependencyCheck {
        host: host_of(url),
        checked: false,
        reachable: false,
        status: None,
        latency_ms: None,
        message: Some("Offline mode: no outside request is made.".to_owned()),
    }
}

/// A health check is served without a refresh for this long.
pub(crate) const HEALTH_FRESH: std::time::Duration = std::time::Duration::from_secs(60);
/// Past this, the last check is no longer shown without waiting for a new one.
pub(crate) const HEALTH_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(10 * 60);

/// Whether two checks found the same thing: when they ran and how long a host took
/// are not findings.
pub(crate) fn same_health(old: &DependencyHealth, new: &DependencyHealth) -> bool {
    let mut new = new.clone();
    new.checked_at.clone_from(&old.checked_at);
    new.github.latency_ms = old.github.latency_ms;
    for (new, old) in new.providers.iter_mut().zip(&old.providers) {
        if let (Some(new), Some(old)) = (&mut new.service, &old.service) {
            new.latency_ms = old.latency_ms;
        }
    }
    *old == new
}

impl Core {
    /// Checks GitHub and each installed provider's service. In offline mode nothing is
    /// sent, and every check says it was not made. The last check answers at once and is
    /// made again in the background, announcing `health` when something changed;
    /// `refresh` checks now.
    pub async fn dependency_health(&self, refresh: bool) -> Result<DependencyHealth> {
        let online = self.config.engines.lookup_latest;
        let (http, engines) = (self.http.clone(), self.engines.clone());
        self.health_reads
            .get(
                // Engine installs, updates and changes to the engine set are inputs of the check.
                &crate::util::swr::key(&[
                    "health",
                    if online { "online" } else { "offline" },
                    &self.engines.change_epoch().to_string(),
                ]),
                "health",
                refresh,
                move || async move {
                    let check = |url: &'static str| {
                        let http = http.clone();
                        async move {
                            if online {
                                check_url(&http, url).await
                            } else {
                                not_checked(url)
                            }
                        }
                    };
                    let github = check(GITHUB).await;
                    let mut providers = Vec::new();
                    for status in engines.statuses(false).await? {
                        let family = status
                            .instance_of
                            .clone()
                            .unwrap_or_else(|| status.id.clone());
                        let service = match provider_service(&family) {
                            Some(url) if status.installed => Some(check(url).await),
                            _ => None,
                        };
                        providers.push(ProviderHealth {
                            engine: status.id,
                            name: status.name,
                            installed: status.installed,
                            auth: status.auth,
                            problem: status.problem,
                            service,
                        });
                    }
                    Ok(DependencyHealth {
                        checked_at: now(),
                        github,
                        providers,
                    })
                },
            )
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    async fn answering(status_line: &'static str) -> String {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let mut buffer = [0; 1024];
                let _ = socket.read(&mut buffer).await;
                let reply = format!("HTTP/1.1 {status_line}\r\ncontent-length: 0\r\n\r\n");
                let _ = socket.write_all(reply.as_bytes()).await;
            }
        });
        format!("http://{address}/")
    }

    #[tokio::test]
    async fn any_http_answer_is_reachable_even_an_error_status() {
        let url = answering("404 Not Found").await;
        let check = check_url(&reqwest::Client::new(), &url).await;
        assert!(check.reachable && check.checked);
        assert_eq!(check.status, Some(404));
        assert!(check.latency_ms.is_some() && check.message.is_none());
    }

    #[tokio::test]
    async fn a_host_that_does_not_answer_is_unreachable_with_a_reason() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/", listener.local_addr().unwrap());
        drop(listener);
        let check = check_url(&reqwest::Client::new(), &url).await;
        assert!(!check.reachable && check.checked);
        assert!(check.status.is_none() && check.message.is_some());
    }

    #[test]
    fn only_claude_and_codex_families_have_a_service_host() {
        assert_eq!(
            provider_service("claude"),
            Some("https://api.anthropic.com/")
        );
        assert_eq!(provider_service("codex"), Some("https://api.openai.com/"));
        assert_eq!(provider_service("my-acp"), None);
    }

    #[tokio::test]
    async fn a_kept_check_answers_at_once_and_a_forced_one_checks_again() {
        let dir = crate::testing::TempDir::new();
        let mut config =
            crate::app::CoreConfig::new(dir.path().join("data"), "http://127.0.0.1:1".into());
        config.engines.lookup_latest = false;
        let core = Core::open(config).unwrap();
        let first = core.dependency_health(false).await.unwrap();
        assert!(!first.github.checked, "offline mode sends nothing");
        let mut kept = first.clone();
        kept.checked_at = "kept".into();
        let epoch = core.engines.change_epoch().to_string();
        core.health_reads
            .put(&crate::util::swr::key(&["health", "offline", &epoch]), kept);
        let started = std::time::Instant::now();
        for _ in 0..100 {
            assert_eq!(
                core.dependency_health(false).await.unwrap().checked_at,
                "kept"
            );
        }
        assert!(started.elapsed() < std::time::Duration::from_millis(500));
        assert_ne!(
            core.dependency_health(true).await.unwrap().checked_at,
            "kept"
        );
    }

    #[test]
    fn a_new_time_or_latency_is_not_a_change() {
        let check = |latency| DependencyCheck {
            host: "github.com".into(),
            checked: true,
            reachable: true,
            status: Some(200),
            latency_ms: Some(latency),
            message: None,
        };
        let health = |at: &str, latency, reachable| {
            let mut github = check(latency);
            github.reachable = reachable;
            DependencyHealth {
                checked_at: at.into(),
                github,
                providers: Vec::new(),
            }
        };
        assert!(same_health(&health("a", 10, true), &health("b", 900, true)));
        assert!(!same_health(
            &health("a", 10, true),
            &health("a", 10, false)
        ));
    }
}
