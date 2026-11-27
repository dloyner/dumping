//! Same-origin vs same-site
//!
//! Big idea:
//!   - same-origin = same scheme + same host + same port   (used by SOP and CORS)
//!   - same-site   = same scheme + same registrable domain (eTLD+1)
//!                   (used by SameSite cookies). Port and subdomain are ignored.
//!
//! Same-origin is stricter: same-origin implies same-site, not the other way round.
//! Caveat: sibling subdomains are same-site, so SameSite does NOT protect you from
//! a compromised `blog.example.com`.
//!
//! Simplified on purpose: tiny hardcoded Public Suffix List and a hand-written
//! URL parser. Real code uses the `url` and `publicsuffix` crates.

/// A tiny sample of the Public Suffix List (suffixes anyone can register under).
const PUBLIC_SUFFIXES: &[&str] = &["com", "org", "net", "co.uk", "github.io"];

#[derive(Debug, PartialEq, Eq)]
pub struct UrlParts {
    pub scheme: String,
    pub host: String,
    pub port: u16,
}

fn default_port(scheme: &str) -> Option<u16> {
    match scheme {
        "http" => Some(80),
        "https" => Some(443),
        _ => None,
    }
}

/// Parse `scheme://host[:port][/...]`. IPv6 literals are only partially handled.
pub fn parse_url(url: &str) -> Option<UrlParts> {
    let (scheme, rest) = url.split_once("://")?;
    let authority = rest.split(['/', '?', '#']).next()?;
    let (host, port) = match authority.rsplit_once(':') {
        Some((h, p)) => (h, p.parse().ok()?),
        None => (authority, default_port(scheme)?),
    };
    let host = host.trim_start_matches('[').trim_end_matches(']');
    if host.is_empty() {
        return None;
    }
    Some(UrlParts { scheme: scheme.to_string(), host: host.to_string(), port })
}

/// eTLD+1: the public suffix plus one more label on its left.
pub fn registrable_domain(host: &str) -> String {
    let labels: Vec<&str> = host.split('.').collect();
    // Walk from the longest candidate suffix to the shortest.
    for i in 0..labels.len() {
        if PUBLIC_SUFFIXES.contains(&labels[i..].join(".").as_str()) && i > 0 {
            return labels[i - 1..].join(".");
        }
    }
    host.to_string() // e.g. "localhost"
}

pub fn same_origin(a: &str, b: &str) -> bool {
    matches!((parse_url(a), parse_url(b)), (Some(x), Some(y)) if x == y)
}

pub fn same_site(a: &str, b: &str) -> bool {
    match (parse_url(a), parse_url(b)) {
        (Some(x), Some(y)) => {
            x.scheme == y.scheme && registrable_domain(&x.host) == registrable_domain(&y.host)
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const APP: &str = "https://app.example.com";

    #[test]
    fn subdomain_port_and_path_do_not_matter_for_same_site() {
        assert!(same_site(APP, "https://api.example.com"));
        assert!(same_site(APP, "https://example.com:3000/other"));
    }

    #[test]
    fn scheme_and_domain_do_matter() {
        assert!(!same_site(APP, "http://app.example.com"));
        assert!(!same_site(APP, "https://example.org"));
    }

    #[test]
    fn public_suffix_splits_sites() {
        assert!(!same_site("https://alice.github.io", "https://bob.github.io"));
        assert_eq!(registrable_domain("shop.example.co.uk"), "example.co.uk");
    }

    #[test]
    fn same_origin_is_stricter_than_same_site() {
        assert!(same_site(APP, "https://api.example.com"));
        assert!(!same_origin(APP, "https://api.example.com"));
        assert!(same_origin(APP, "https://app.example.com:443/x"));
    }
}
