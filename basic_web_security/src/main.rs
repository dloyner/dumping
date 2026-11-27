//! Wires every module into one axum app so you can poke at it with curl.
//! See README.md for commands to try.

use std::net::SocketAddr;

use axum::{Router, middleware, routing::get};
use basic_web_security::{
    cors,
    csrf::{self, AllowedOrigin},
    dos::{self, RateLimiter},
    headers, rest, session, ssrf, xss,
};

/// The only browser origin we trust (our own frontend / this server).
const ORIGIN: &str = "http://localhost:3000";

#[tokio::main]
async fn main() {
    let routes = Router::new()
        .route("/", get(|| async { "basic_web_security: see README.md" }))
        .merge(xss::router())
        .merge(session::router())
        .merge(rest::router())
        .merge(ssrf::router());

    // In axum the LAST `.layer` is the OUTERMOST one, so a request passes through
    // them from the bottom of this chain to the top:
    //   rate limit -> security headers -> CORS -> CSRF guard -> size/time limits -> handler
    let app = dos::with_limits(routes)
        .layer(middleware::from_fn_with_state(AllowedOrigin(ORIGIN), csrf::guard))
        .layer(cors::layer(ORIGIN))
        .layer(middleware::from_fn(headers::add_security_headers))
        // Cheapest rejection first: 20-request burst, 10 requests/second per IP.
        .layer(middleware::from_fn_with_state(RateLimiter::new(20.0, 10.0), dos::rate_limit));

    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await.expect("bind failed");
    println!("listening on {ORIGIN}");

    // ConnectInfo gives the rate limiter the peer address.
    axum::serve(listener, app.into_make_service_with_connect_info::<SocketAddr>())
        .await
        .expect("server error");
}
