//! SSRF (Server-Side Request Forgery)
//!
//! Big idea: the attacker makes YOUR SERVER send a request to an address they
//! choose. The request carries the server's network position and trust, so it can
//! reach internal services, `localhost`, or the cloud metadata endpoint
//! (169.254.169.254 on AWS = IAM credentials). Compare: CSRF abuses the user's
//! browser, SSRF abuses the server.
//!
//! Defense checklist:
//!   1. Prefer an allowlist of hosts.
//!   2. Otherwise resolve DNS FIRST, then validate the resolved IPs (never match
//!      on the URL string: `127.1`, `0x7f000001`, `localtest.me` all bypass it).
//!   3. Only http/https. 4. Do not follow redirects blindly.
//!   5. Connect to the validated IP (prevents DNS rebinding).
//!   6. On AWS: enforce IMDSv2 and least-privilege IAM roles.
//!
//! This demo only resolves and validates; it never performs the request.

use std::net::{IpAddr, SocketAddr};

use axum::{Router, extract::Query, http::StatusCode, routing::get};
use serde::Deserialize;
use tokio::net::lookup_host;

use crate::site::parse_url;

/// Public means: not loopback, private, link-local, CGNAT, multicast, unspecified...
pub fn is_public_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            let o = v4.octets();
            let cgnat = o[0] == 100 && (o[1] & 0xc0) == 64; // 100.64.0.0/10
            !(v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local() // 169.254.0.0/16 (cloud metadata)
                || v4.is_unspecified()
                || v4.is_broadcast()
                || v4.is_multicast()
                || cgnat)
        }
        IpAddr::V6(v6) => {
            // `::ffff:127.0.0.1` is an IPv4 address in disguise.
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_public_ip(IpAddr::V4(v4));
            }
            let seg0 = v6.segments()[0];
            let unique_local = (seg0 & 0xfe00) == 0xfc00; // fc00::/7
            let link_local = (seg0 & 0xffc0) == 0xfe80; // fe80::/10
            !(v6.is_loopback()
                || v6.is_unspecified()
                || v6.is_multicast()
                || unique_local
                || link_local)
        }
    }
}

/// Resolve the host once and reject it if ANY resolved address is non-public.
/// Return the validated address so the caller can connect to exactly that IP.
pub async fn resolve_and_validate(raw: &str) -> Result<SocketAddr, &'static str> {
    let url = parse_url(raw).ok_or("invalid url")?;
    if !matches!(url.scheme.as_str(), "http" | "https") {
        return Err("scheme not allowed");
    }
    let addrs: Vec<SocketAddr> = lookup_host((url.host.as_str(), url.port))
        .await
        .map_err(|_| "dns resolution failed")?
        .collect();
    if addrs.iter().any(|a| !is_public_ip(a.ip())) {
        return Err("host resolves to a non-public address");
    }
    addrs.first().copied().ok_or("no address found")
}

#[derive(Deserialize)]
struct FetchQuery {
    url: String,
}

async fn fetch(Query(p): Query<FetchQuery>) -> Result<String, (StatusCode, String)> {
    match resolve_and_validate(&p.url).await {
        Ok(addr) => Ok(format!("allowed: would connect to {addr}")),
        Err(why) => Err((StatusCode::BAD_REQUEST, format!("blocked: {why}"))),
    }
}

pub fn router() -> Router {
    Router::new().route("/ssrf/fetch", get(fetch))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ip(s: &str) -> IpAddr {
        s.parse().unwrap()
    }

    #[test]
    fn internal_addresses_are_rejected() {
        for bad in [
            "127.0.0.1",
            "10.0.0.5",
            "192.168.1.1",
            "169.254.169.254",
            "::1",
            "fc00::1",
            "::ffff:127.0.0.1",
        ] {
            assert!(!is_public_ip(ip(bad)), "{bad} should be blocked");
        }
    }

    #[test]
    fn public_addresses_are_allowed() {
        assert!(is_public_ip(ip("8.8.8.8")));
        assert!(is_public_ip(ip("2001:4860:4860::8888")));
    }

    #[tokio::test]
    async fn metadata_and_bad_schemes_are_blocked() {
        assert!(
            resolve_and_validate("http://169.254.169.254/latest/meta-data")
                .await
                .is_err()
        );
        assert!(resolve_and_validate("http://127.0.0.1:6379").await.is_err());
        assert!(resolve_and_validate("file:///etc/passwd").await.is_err());
    }
}
