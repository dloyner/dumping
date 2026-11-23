use std::time::Duration;
use std::{net::SocketAddr, sync::Arc};

use axum::{
    Router,
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use rust_embed::RustEmbed;
use tower_governor::{GovernorLayer, governor::GovernorConfigBuilder};
#[cfg(debug_assertions)]
use tower_http::cors::{Any, CorsLayer};
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tracing::Level;

use auth_dal::{
    migrations::run_migrations as run_auth_migrations,
    users::descriptors::SqlxPostgresDescriptor as AuthPostgresDescriptor,
};
use auth_server::api::RedisSessionDescriptor;
use glue::{
    db::pool_from_env,
    log::{DefaultMakeSpan, DefaultOnRequest, DefaultOnResponse, TraceLayer, logger::init_logger},
};
use to_do_axum_server::api as todo_api;
use to_do_dal::migrations::run_migrations as run_todo_migrations;
use to_do_dal::to_do_items::descriptors::SqlxPostgresDescriptor;

const INDEX_HTML: &str = include_str!("../../frontend/public/index.html");
const NOT_FOUND: (StatusCode, &str) = (StatusCode::NOT_FOUND, "404 Not Found");

fn rate_limited<S>(router: Router<S>, per_second: u64, burst_size: u32) -> Router<S>
where
    S: Clone + Send + Sync + 'static,
{
    let config = Arc::new(
        GovernorConfigBuilder::default()
            .per_second(per_second)
            .burst_size(burst_size)
            .use_headers()
            .finish()
            .expect("valid auth rate limit"),
    );

    let limiter = config.limiter().clone();
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(Duration::from_secs(60));
        loop {
            interval.tick().await;
            limiter.retain_recent();
        }
    });

    router.layer(GovernorLayer::new(config))
}

#[derive(RustEmbed)]
#[folder = "../frontend/public"]
#[exclude = "*.map"]
struct FrontendAssets;

async fn index() -> Html<&'static str> {
    Html(INDEX_HTML)
}

fn serve_frontend_asset(path: &str) -> Response {
    let Some(asset) = FrontendAssets::get(path) else {
        return NOT_FOUND.into_response();
    };

    let mime = mime_guess::from_path(path)
        .first_or_octet_stream()
        .to_string();

    (
        [
            (header::CONTENT_TYPE, mime),
            (header::CACHE_CONTROL, "no-cache".to_owned()),
        ],
        asset.data,
    )
        .into_response()
}

async fn catch_all(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") {
        return NOT_FOUND.into_response();
    }
    match FrontendAssets::get(path) {
        Some(_) => serve_frontend_asset(path),
        None if path.contains('.') => NOT_FOUND.into_response(),
        None => Html(INDEX_HTML).into_response(),
    }
}

fn app(todo_store: SqlxPostgresDescriptor, auth_store: AuthPostgresDescriptor) -> Router {
    type Session = RedisSessionDescriptor<AuthPostgresDescriptor>;

    let auth_state = auth_server::api::AppState::new(
        auth_store.clone(),
        RedisSessionDescriptor::new(auth_store.clone()),
    );
    let auth_routes = Router::new()
        .merge(rate_limited(
            auth_server::api::auth::auth_router::<AuthPostgresDescriptor, Session>(),
            12,
            5,
        ))
        .merge(rate_limited(
            auth_server::api::users::create_router::<AuthPostgresDescriptor, Session>(),
            3_600,
            3,
        ))
        .merge(auth_server::api::users::get_router::<
            AuthPostgresDescriptor,
            Session,
        >());
    let auth_routes = Router::new()
        .nest("/api/v1", auth_routes)
        .with_state(auth_state);

    let app = Router::new()
        .route("/", get(index))
        .route("/healthz", get(|| async { StatusCode::OK }))
        .merge(todo_api::basic_actions::basic_actions_router_core_postgres(
            todo_store,
            auth_store.clone(),
        ))
        .merge(auth_routes)
        .fallback(catch_all)
        .layer(
            TraceLayer::new_for_http()
                .make_span_with(DefaultMakeSpan::new().level(Level::INFO))
                .on_request(DefaultOnRequest::new().level(Level::INFO))
                .on_response(DefaultOnResponse::new().level(Level::INFO)),
        );
    let app = app.layer(TimeoutLayer::with_status_code(
        StatusCode::REQUEST_TIMEOUT,
        Duration::from_secs(30),
    ));
    let app = app.layer(SetResponseHeaderLayer::if_not_present(
        header::CONTENT_SECURITY_POLICY,
        "default-src 'self'; script-src 'self' 'wasm-unsafe-eval'; style-src 'self'; img-src 'self' data:; connect-src 'self'; object-src 'none'; base-uri 'self'; frame-ancestors 'none'"
            .parse::<axum::http::HeaderValue>()
            .unwrap(),
    ));
    #[cfg(debug_assertions)]
    let app = app.layer(
        CorsLayer::new()
            .allow_origin(
                std::env::var("FRONTEND_DEV_ORIGIN")
                    .unwrap_or_else(|_| "http://localhost:3000".to_string())
                    .parse::<axum::http::HeaderValue>()
                    .unwrap(),
            )
            .allow_methods(Any)
            .allow_headers(Any),
    );
    app
}

async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let term = async {
        if let Ok(mut s) = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            s.recv().await;
        }
    };
    #[cfg(not(unix))]
    let term = std::future::pending::<()>();
    tokio::select! { _ = ctrl_c => {}, _ = term => {} }
    tracing::info!("shutdown signal received");
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    dotenvy::dotenv().ok();
    init_logger();
    glue::token::HeaderToken::validate_secret()?;
    auth_kernel::cache_api_url()?;
    let pool = pool_from_env("TO_DO")?;
    run_todo_migrations(&pool).await?;
    let auth_pool = pool_from_env("AUTH")?;
    run_auth_migrations(&auth_pool).await?;

    let host = std::env::var("INGRESS_WEB_HOST").unwrap_or_else(|_| "0.0.0.0".to_string());
    let port = std::env::var("INGRESS_WEB_PORT").unwrap_or_else(|_| "8001".to_string());
    let listener = tokio::net::TcpListener::bind(format!("{host}:{port}")).await?;
    tracing::info!(address = %listener.local_addr()?, "ingress server listening");

    let todo_store = SqlxPostgresDescriptor::new(pool);
    let auth_store = AuthPostgresDescriptor::new(auth_pool);
    axum::serve(
        listener,
        app(todo_store, auth_store).into_make_service_with_connect_info::<SocketAddr>(),
    )
    .with_graceful_shutdown(shutdown_signal())
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;

    use axum::{
        Extension,
        body::Body,
        extract::ConnectInfo,
        http::{Method, Request, StatusCode},
    };
    use tower::ServiceExt;

    use super::{AuthPostgresDescriptor, app};
    use glue::db::create_pool;
    use to_do_dal::to_do_items::descriptors::SqlxPostgresDescriptor;

    fn test_app() -> axum::Router {
        let todo_pool = create_pool("postgres://test:test@127.0.0.1/todo_test", 1).unwrap();
        let auth_pool = create_pool("postgres://test:test@127.0.0.1/auth_test", 1).unwrap();
        app(
            SqlxPostgresDescriptor::new(todo_pool),
            AuthPostgresDescriptor::new(auth_pool),
        )
        .layer(Extension(ConnectInfo(SocketAddr::from((
            [127, 0, 0, 1],
            3000,
        )))))
    }

    #[tokio::test]
    async fn serves_the_frontend_shell_and_assets() {
        let app = test_app();
        let health = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(health.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/html")
        );
        let spa_route = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/tasks/42")
                    .header("accept", "text/html")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(spa_route.status(), StatusCode::OK);
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/bundle.css")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert!(
            response.headers()["content-type"]
                .to_str()
                .unwrap()
                .starts_with("text/css")
        );
        let missing_asset = app
            .oneshot(
                Request::builder()
                    .uri("/missing.js")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_asset.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn routes_api_requests_and_returns_404_for_unknown_api_paths() {
        let invalid_json = test_app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/create")
                    .header("content-type", "application/json")
                    .body(Body::from("not-json"))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_json.status(), StatusCode::UNAUTHORIZED);

        let missing_route = test_app()
            .oneshot(
                Request::builder()
                    .uri("/api/v1/unknown")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_route.status(), StatusCode::NOT_FOUND);

        let cors = test_app()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/api/v1/create")
                    .header("origin", "http://localhost:3000")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(cors.status(), StatusCode::OK);
        assert_eq!(
            cors.headers()["access-control-allow-origin"],
            "http://localhost:3000"
        );
    }

    #[tokio::test]
    async fn logout_requires_a_valid_token_header() {
        static INIT_SECRET: std::sync::Once = std::sync::Once::new();
        INIT_SECRET.call_once(|| unsafe {
            std::env::set_var("JWT_SECRET", "ingress-test-secret-32-bytes-long");
        });

        let missing_token = test_app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/logout")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(missing_token.status(), StatusCode::UNAUTHORIZED);

        let invalid_token = test_app()
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/logout")
                    .header("Authorization", "Bearer not-a-jwt")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(invalid_token.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn auth_routes_are_rate_limited_by_peer_ip() {
        let app = test_app();
        for _ in 0..5 {
            let response = app
                .clone()
                .oneshot(
                    Request::builder()
                        .method(Method::POST)
                        .uri("/api/v1/auth/login")
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        }

        let limited = app
            .oneshot(
                Request::builder()
                    .method(Method::POST)
                    .uri("/api/v1/auth/login")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(limited.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(limited.headers().contains_key("retry-after"));
    }
}
