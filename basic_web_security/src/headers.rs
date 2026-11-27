//! Security response headers (clickjacking, CSP, ...)
//!
//! Big idea: clickjacking = attacker embeds your site in a transparent `<iframe>`
//! over a fake button, so the victim clicks YOUR button while logged in.
//! Fix on the site being embedded: `frame-ancestors` (CSP) / `X-Frame-Options`.
//! Fix on the site doing the embedding of untrusted content: `<iframe sandbox>`.
//!
//! CSP also limits where scripts may load from, which reduces the damage of XSS.

use axum::{
    extract::Request,
    http::{HeaderValue, header},
    middleware::Next,
    response::Response,
};

pub async fn add_security_headers(req: Request, next: Next) -> Response {
    let mut res = next.run(req).await;
    let h = res.headers_mut();
    // No inline scripts, same-origin resources only, nobody may frame us.
    h.insert(
        header::CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("default-src 'self'; frame-ancestors 'none'"),
    );
    // Legacy equivalent of frame-ancestors, kept for old browsers.
    h.insert(header::X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    // Do not let the browser guess content types.
    h.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    h.insert(header::REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    res
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{Router, body::Body, middleware, routing::get};
    use tower::ServiceExt;

    #[tokio::test]
    async fn responses_forbid_framing() {
        let app = Router::new()
            .route("/", get(|| async { "hi" }))
            .layer(middleware::from_fn(add_security_headers));
        let res = app
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        let csp = res.headers()[header::CONTENT_SECURITY_POLICY].to_str().unwrap();
        assert!(csp.contains("frame-ancestors 'none'"));
        assert_eq!(res.headers()[header::X_FRAME_OPTIONS], "DENY");
    }
}
