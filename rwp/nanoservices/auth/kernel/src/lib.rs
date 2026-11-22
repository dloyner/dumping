pub mod api;

use std::sync::OnceLock;

use glue::errors::{NanoServiceError, NanoServiceErrorStatus};

static CACHE_API_URL: OnceLock<String> = OnceLock::new();

pub fn cache_api_url() -> Result<&'static str, NanoServiceError> {
    if let Some(address) = CACHE_API_URL.get() {
        return Ok(address);
    }
    let address = std::env::var("CACHE_API_URL").map_err(|error| {
        NanoServiceError::new(error.to_string(), NanoServiceErrorStatus::Unknown)
    })?;
    Ok(CACHE_API_URL.get_or_init(|| address))
}

#[cfg(any(feature = "core-postgres", feature = "http"))]
pub mod user_session;
