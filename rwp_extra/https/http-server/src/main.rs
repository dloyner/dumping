use std::net::SocketAddr;

use axum::{Json, Router, routing::post};
use axum_server::tls_rustls::RustlsConfig;
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize, Debug)]
struct InputData {
    message: String,
}

async fn post_handler(Json(data): Json<InputData>) -> Json<Value> {
    Json(json!({ "response": format!("Received: {}", data.message) }))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = RustlsConfig::from_pem_file("cert.pem", "key.pem").await?;
    let app = Router::new().route("/api/data", post(post_handler));
    let addr = SocketAddr::from(([127, 0, 0, 1], 8080));

    axum_server::bind_rustls(addr, cfg)
        .serve(app.into_make_service())
        .await?;

    Ok(())
}
