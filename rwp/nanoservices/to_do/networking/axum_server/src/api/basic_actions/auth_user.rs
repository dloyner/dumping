use auth_kernel::user_session::transactions::get::GetUserSession;
use axum::{extract::FromRequestParts, http::request::Parts};
use glue::{errors::NanoServiceError, token::HeaderToken};

use crate::api::AppState;

pub struct AuthUser(pub String);

impl<T, X> FromRequestParts<AppState<T, X>> for AuthUser
where
    T: Send + Sync,
    X: GetUserSession + Send + Sync,
{
    type Rejection = NanoServiceError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState<T, X>,
    ) -> Result<Self, Self::Rejection> {
        let token = HeaderToken::from_request_parts(parts, state).await?;
        let session = state
            .user_session
            .get_user_session(&token.unique_id)
            .await?;
        Ok(Self(session.user_id.to_string()))
    }
}
