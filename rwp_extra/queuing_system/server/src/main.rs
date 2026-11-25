use std::net::SocketAddr;
use std::sync::Arc;

use axum::{Json, Router, extract::State, http::StatusCode, routing::post};
use redis::{AsyncCommands, Client};

use kernel::Payload;

const QUEUE: &str = "payload_queue";

#[derive(Clone)]
struct AppState {
    redis_client: Client,
}

#[tokio::main]
async fn main() {
    let redis_client = Client::open("redis://127.0.0.1/").expect("Failed to connect to redis");
    let app_state = Arc::new(AppState { redis_client });

    let app = Router::new()
        .route("/submit", post(write_to_redis))
        .with_state(app_state);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    let listener = tokio::net::TcpListener::bind(addr).await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn write_to_redis(
    State(state): State<Arc<AppState>>,
    Json(payload): Json<Payload>,
) -> Result<&'static str, (StatusCode, String)> {
    let mut connection = state
        .redis_client
        .get_multiplexed_async_connection()
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    let payload_json: String = serde_json::to_string(&payload)
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    connection
        .lpush::<_, _, ()>(QUEUE, payload_json)
        .await
        .map_err(|err| (StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;

    Ok("payload submitted to Redis queue")
}
