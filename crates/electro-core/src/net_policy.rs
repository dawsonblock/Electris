use serde::{Deserialize, Serialize};

/// Policy defining the permitted network egress classes for a tool.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub enum NetworkPolicy {
    /// No outbound network access is allowed.
    #[default]
    Blocked,
    /// Egress is restricted exclusively to public web addresses (no RFC1918/localhost).
    /// If an allowlist is provided, the target must also end in one of the permitted domains.
    PublicWeb { allowlist: Option<Vec<String>> },
    /// Anything is allowed (typically reserved for highly trusted internal components).
    Unrestricted,
}

pub const PUBLIC_WEB_ALLOWLIST_ENV: &str = "ELECTRO_PUBLIC_WEB_ALLOWLIST";

impl NetworkPolicy {
    /// Load the domain allowlist from ELECTRO_PUBLIC_WEB_ALLOWLIST and return a PublicWeb policy.
    pub fn public_web_from_env() -> Self {
        let list = std::env::var(PUBLIC_WEB_ALLOWLIST_ENV)
            .unwrap_or_default()
            .split(',')
            .map(|s| s.trim().trim_matches('.').to_ascii_lowercase())
            .filter(|s| !s.is_empty())
            .collect::<Vec<String>>();

        if list.is_empty() {
            NetworkPolicy::PublicWeb { allowlist: None }
        } else {
            NetworkPolicy::PublicWeb {
                allowlist: Some(list),
            }
        }
    }
}

/// Detects if a host string resolves to a private, loopback, link-local, or internal
/// network namespace address.
pub fn host_address_is_blocked_for_public_web(host: &str) -> bool {
    let host = host.trim().to_ascii_lowercase();
    if host.is_empty() {
        return true;
    }

    if matches!(host.as_str(), "localhost" | "localhost.localdomain")
        || host.ends_with(".local")
        || host.ends_with(".internal")
        || host.ends_with(".localhost")
    {
        return true;
    }

    if let Ok(ip) = host.parse::<std::net::IpAddr>() {
        return match ip {
            std::net::IpAddr::V4(ip) => {
                ip.is_private()
                    || ip.is_loopback()
                    || ip.is_link_local()
                    || ip.is_multicast()
                    || ip.is_unspecified()
            }
            std::net::IpAddr::V6(ip) => {
                let seg0 = ip.segments()[0];
                ip.is_loopback()
                    || ip.is_unspecified()
                    || (seg0 & 0xfe00) == 0xfc00
                    || (seg0 & 0xffc0) == 0xfe80
            }
        };
    }

    false
}

/// Check if a hostname matches an allowlist entry (exact or subdomain).
pub fn host_matches_allow_entry(host: &str, entry: &str) -> bool {
    let host = host.trim().trim_matches('.').to_ascii_lowercase();
    let entry = entry.trim().trim_matches('.').to_ascii_lowercase();
    if host.is_empty() || entry.is_empty() {
        return false;
    }
    host == entry || host.ends_with(&format!(".{entry}"))
}
