#[cfg(any(feature = "core-postgres", feature = "http"))]
mod common_imports {
    pub use auth_dal::users::schema::TrimmedUser;
    pub use glue::errors::NanoServiceError;
}

#[cfg(feature = "core-postgres")]
mod core_imports {
    pub use auth_core::api::users::get::get_by_unique_id as get_by_unique_id_core;
    pub use auth_dal::users::transactions::get::GetByUniqueId;
}

#[cfg(feature = "http")]
mod reqwest_imports {
    pub use glue::errors::{NanoServiceErrorStatus, ResultExt};
    pub use glue::token::HeaderToken;
    pub use reqwest::Client;
}

#[cfg(any(feature = "core-postgres", feature = "http"))]
use common_imports::*;
#[cfg(feature = "http")]
use std::sync::OnceLock;

#[cfg(feature = "core-postgres")]
use core_imports::*;

#[cfg(feature = "http")]
use reqwest_imports::*;

#[cfg(feature = "core-postgres")]
pub async fn get_user_by_unique_id_core<T: GetByUniqueId>(
    store: &T,
    id: &str,
) -> Result<TrimmedUser, NanoServiceError> {
    get_by_unique_id_core(store, id).await
}

#[cfg(feature = "http")]
pub async fn get_user_by_unique_id_http(id: &str) -> Result<TrimmedUser, NanoServiceError> {
    get_user_by_unique_id_api_call(id).await
}

#[cfg(feature = "http")]
async fn get_user_by_unique_id_api_call(id: &str) -> Result<TrimmedUser, NanoServiceError> {
    static AUTH_API_URL: OnceLock<String> = OnceLock::new();
    let url = if let Some(url) = AUTH_API_URL.get() {
        url.as_str()
    } else {
        let value = std::env::var("AUTH_API_URL").eject(NanoServiceErrorStatus::Unknown)?;
        AUTH_API_URL.get_or_init(|| value).as_str()
    };
    let full_url = format!("{}/api/v1/users/me", url);
    let header_token = HeaderToken {
        unique_id: id.to_owned(),
    }
    .encode()?;

    let client = Client::new();
    let response = client
        .get(&full_url)
        .header("Authorization", format!("Bearer {header_token}"))
        .send()
        .await
        .eject(NanoServiceErrorStatus::Unknown)?;

    if response.status().is_success() {
        let trimmed_user = response
            .json::<TrimmedUser>()
            .await
            .eject(NanoServiceErrorStatus::Unknown)?;

        Ok(trimmed_user)
    } else {
        Err(NanoServiceError::new(
            format!("failed to get user: {}", response.status()),
            NanoServiceErrorStatus::Unknown,
        ))
    }
}
