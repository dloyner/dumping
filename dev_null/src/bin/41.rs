use reqwest::{Client, Method};
use serde_json::json;

#[tokio::main]
async fn main() -> Result<(), reqwest::Error> {
    let method = Method::from_bytes(b"QUERY").expect("valid method token");

    let resp:  reqwest::Response = Client::new()
        .request(method, "https://example.com/feed")
        .json(&json!({ "q": "foo", "limit": 10, "sort": "-published" }))
        .send()
        .await?;

    println!("{}", resp.text().await?);
    Ok(())
}
