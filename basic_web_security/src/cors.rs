//! CORS (Cross-Origin Resource Sharing)
//!
//! Big idea: the Same-Origin Policy (SOP) stops JS on origin A from READING
//! responses from origin B. CORS is how server B opts in: "origin A may read me".
//!
//! Common misunderstandings:
//!   1. CORS is enforced by the BROWSER. curl/Postman ignore it.
//!   2. CORS is NOT a CSRF defense: the request is still sent and processed,
//!      CORS only blocks JS from reading the response.
//!   3. `Allow-Credentials: true` cannot be combined with `Allow-Origin: *`.
//!
//! Preflight: for "non-simple" requests (PUT/DELETE, `Authorization` header,
//! `Content-Type: application/json`) the browser first sends `OPTIONS` and only
//! sends the real request if the server agrees.

use std::time::Duration;

use axum::http::{HeaderValue, Method, header};
use tower_http::cors::CorsLayer;

pub fn layer(allowed_origin: &'static str) -> CorsLayer {
    CorsLayer::new()
        // One explicit origin (required because credentials are allowed).
        .allow_origin(HeaderValue::from_static(allowed_origin))
        .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE])
        .allow_headers([header::AUTHORIZATION, header::CONTENT_TYPE])
        .allow_credentials(true)
        // Let the browser cache the preflight answer.
        .max_age(Duration::from_secs(600))
}
