//! Shared network boundary helpers for public-web tools.
//!
//! These helpers enforce the repo's public-web policy:
//! - only http/https URLs
//! - no loopback/private/link-local/internal/local targets
//! - optional operator allowlist via environment variable
//! - DNS resolution must stay on public addresses

pub use electro_core::net_policy::host_address_is_blocked_for_public_web as host_is_blocked;
use url::Url;

pub use electro_core::net_policy::host_matches_allow_entry;
pub use electro_core::net_policy::PUBLIC_WEB_ALLOWLIST_ENV;

/// Split and normalize a comma-separated allowlist of hostnames from a specific env var.
pub fn load_domain_allowlist_from_env(var_name: &str) -> Vec<String> {
    std::env::var(var_name)
        .unwrap_or_default()
        .split(',')
        .map(|s| s.trim().trim_matches('.').to_ascii_lowercase())
        .filter(|s| !s.is_empty())
        .collect()
}

pub fn enforce_host_allowlist(host: &str, allowlist: &[String], label: &str) -> Result<(), String> {
    if allowlist.is_empty() {
        return Ok(());
    }

    if allowlist
        .iter()
        .any(|entry| host_matches_allow_entry(host, entry))
    {
        Ok(())
    } else {
        Err(format!(
            "Blocked {} '{}'. ELECTRO_PUBLIC_WEB_ALLOWLIST is set and this host is not permitted.",
            label, host
        ))
    }
}

/// Build a standard reqwest::Client configured with standard timeouts and redirect policies.
pub fn build_standard_client(
    policy: &electro_core::net_policy::NetworkPolicy,
) -> Result<reqwest::Client, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .user_agent("ELECTRO/0.1");

    match policy {
        electro_core::net_policy::NetworkPolicy::Blocked => {
            builder = builder.redirect(reqwest::redirect::Policy::none());
        }
        electro_core::net_policy::NetworkPolicy::PublicWeb { allowlist } => {
            let allowlist = allowlist.clone();
            builder = builder.redirect(reqwest::redirect::Policy::custom(move |attempt| {
                let host = attempt.url().host_str().map(|s| s.to_owned());
                match host {
                    Some(h) if host_is_blocked(&h) => {
                        attempt.error(format!("Blocked redirect to private/internal host: {h}"))
                    }
                    Some(h) => {
                        if let Some(ref list) = allowlist {
                            if !list.is_empty()
                                && !list.iter().any(|entry| host_matches_allow_entry(&h, entry))
                            {
                                return attempt.error(format!(
                                    "Blocked redirect: {h} is not in the allowlist"
                                ));
                            }
                        }
                        attempt.follow()
                    }
                    None => attempt.error("Redirect target missing host"),
                }
            }));
        }
        electro_core::net_policy::NetworkPolicy::Unrestricted => {
            builder = builder.redirect(reqwest::redirect::Policy::default());
        }
    }

    builder
        .build()
        .map_err(|e| format!("Failed to build reqwest client: {}", e))
}

pub fn validate_public_url(raw: &str) -> Result<Url, String> {
    let url = Url::parse(raw).map_err(|e| format!("Invalid URL: {}", e))?;
    match url.scheme() {
        "http" | "https" => {}
        other => {
            return Err(format!(
                "Unsupported URL scheme '{}'. Only http and https are allowed.",
                other
            ));
        }
    }

    let host = url
        .host_str()
        .ok_or_else(|| "URL must include a host".to_string())?;
    if host_is_blocked(host) {
        return Err(format!(
            "Blocked URL host '{}'. Private, loopback, local, and internal targets are disabled.",
            host
        ));
    }

    Ok(url)
}

fn resolved_port(url: &Url) -> u16 {
    url.port_or_known_default()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 })
}

pub async fn ensure_resolved_host_is_public(url: &Url) -> Result<(), String> {
    let host = url
        .host_str()
        .ok_or_else(|| "URL must include a host".to_string())?;
    let port = resolved_port(url);
    let resolved = tokio::net::lookup_host((host, port))
        .await
        .map_err(|e| format!("Failed to resolve host '{}': {}", host, e))?;

    let mut found = false;
    for addr in resolved {
        found = true;
        let ip = addr.ip();
        if host_is_blocked(&ip.to_string()) {
            return Err(format!(
                "Blocked URL host '{}'. DNS resolved to non-public address {}.",
                host, ip
            ));
        }
    }

    if !found {
        return Err(format!(
            "Failed to resolve host '{}': no addresses returned.",
            host
        ));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_allow_entry_matches_exact_and_subdomain() {
        assert!(host_matches_allow_entry("docs.github.com", "github.com"));
        assert!(host_matches_allow_entry("github.com", "github.com"));
        assert!(!host_matches_allow_entry("evilgithub.com", "github.com"));
    }

    #[test]
    fn load_allowlist_normalizes() {
        std::env::set_var("ELECTRO_TEST_ALLOW", " GitHub.com, docs.rs ,,EXAMPLE.ORG. ");
        let values = load_domain_allowlist_from_env("ELECTRO_TEST_ALLOW");
        assert_eq!(values, vec!["github.com", "docs.rs", "example.org"]);
        std::env::remove_var("ELECTRO_TEST_ALLOW");
    }
}
