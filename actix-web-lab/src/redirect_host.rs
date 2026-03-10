use std::collections::BTreeSet;

use actix_web::{HttpResponse, http::StatusCode};

/// Host allowlist for redirect middleware.
///
/// Pass this type to redirect middleware setter methods such as
/// [`crate::middleware::RedirectHttps::allow_hosts`] to require request hosts to match a known
/// allowlisted value before constructing an absolute redirect target.
///
/// Host matching is case-insensitive. Include ports in allowlist entries when your deployment
/// expects them.
#[derive(Debug, Clone, Default)]
pub struct HostAllowlist {
    hosts: BTreeSet<String>,
}

impl HostAllowlist {
    /// Creates a new host allowlist.
    pub fn new<I, S>(hosts: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        Self {
            hosts: hosts
                .into_iter()
                .map(|host| normalize_host(host.into()))
                .collect(),
        }
    }

    /// Returns true if the host is contained in the allowlist.
    pub fn contains(&self, host: &str) -> bool {
        self.hosts.contains(&normalize_host(host))
    }
}

pub(crate) fn reject_untrusted_host(
    configured_allowlist: Option<&HostAllowlist>,
    host: &str,
) -> Option<HttpResponse<()>> {
    if configured_allowlist.is_some_and(|allowlist| !allowlist.contains(host)) {
        return Some(HttpResponse::with_body(StatusCode::BAD_REQUEST, ()));
    }

    None
}

fn normalize_host(host: impl AsRef<str>) -> String {
    host.as_ref().trim().to_ascii_lowercase()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn host_matching_is_case_insensitive() {
        let allowlist = HostAllowlist::new(["Example.COM:8443"]);

        assert!(allowlist.contains("example.com:8443"));
        assert!(allowlist.contains("EXAMPLE.COM:8443"));
        assert!(!allowlist.contains("example.com"));
    }
}
