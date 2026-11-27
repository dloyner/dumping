//! CSRF (Cross-Site Request Forgery)
//!
//! Big idea: the browser attaches cookies automatically, so a malicious site can
//! make the victim's browser send a request to YOUR site WITH the victim's cookie.
//! The attacker cannot read the response, but the action still happens.
//!
//! Why GET is worse: `<img src=...>` fires a GET with no JS, and `SameSite=Lax`
//! still sends cookies on top-level GET navigation. => never change state with GET.
//!
//! Defenses (layer them): SameSite cookie, CSRF token, check `Origin` /
//! `Sec-Fetch-Site`. Bearer tokens in headers are not auto-sent, so no CSRF.
//! Note: XSS defeats all of these, so fix XSS first.

use axum::{
    extract::{Request, State},
    http::{HeaderMap, Method, StatusCode, header},
    middleware::Next,
    response::Response,
};

/// The single origin allowed to make state-changing requests.
#[derive(Clone)]
pub struct AllowedOrigin(pub &'static str);

/// Pure decision logic (easy to unit test).
pub fn is_allowed(
    method: &Method,
    sec_fetch_site: Option<&str>,
    origin: Option<&str>,
    allowed: &str,
) -> bool {
    // Safe methods must not change state, so we do not guard them.
    let is_safe = *method == Method::GET || *method == Method::HEAD || *method == Method::OPTIONS;
    if is_safe {
        return true;
    }

    // Fetch Metadata is set by the browser and cannot be forged by JS.
    // `same-site` is rejected on purpose: it includes sibling subdomains.
    match sec_fetch_site {
        Some("same-origin" | "none") => return true,
        Some(_) => return false,
        None => {}
    }

    // Fallback for older browsers: compare the Origin header.
    match origin {
        Some(o) => o == allowed,
        // No browser headers at all: curl, server-to-server. Not a CSRF vector.
        None => true,
    }
}

fn header_str<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers.get(name).and_then(|v| v.to_str().ok())
}

/// axum middleware wrapper around `is_allowed`.
pub async fn guard(
    State(allowed): State<AllowedOrigin>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let ok = is_allowed(
        req.method(),
        header_str(req.headers(), "sec-fetch-site"),
        header_str(req.headers(), header::ORIGIN.as_str()),
        allowed.0,
    );
    if ok {
        Ok(next.run(req).await)
    } else {
        Err(StatusCode::FORBIDDEN)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    const OK: &str = "https://app.example.com";

    #[test]
    fn get_is_never_blocked() {
        assert!(is_allowed(&Method::GET, Some("cross-site"), None, OK));
    }

    #[test]
    fn cross_site_post_is_blocked() {
        assert!(!is_allowed(&Method::POST, Some("cross-site"), None, OK));
        assert!(!is_allowed(&Method::POST, Some("same-site"), None, OK));
    }

    #[test]
    fn same_origin_post_is_allowed() {
        assert!(is_allowed(&Method::POST, Some("same-origin"), None, OK));
    }

    #[test]
    fn origin_fallback_for_old_browsers() {
        assert!(is_allowed(&Method::POST, None, Some(OK), OK));
        assert!(!is_allowed(&Method::POST, None, Some("https://evil.example"), OK));
    }
}
