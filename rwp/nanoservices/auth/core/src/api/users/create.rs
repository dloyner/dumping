use serde::Deserialize;

use auth_dal::users::schema::{NewUser, User};
use auth_dal::users::transactions::create::SaveOne;
use glue::errors::{NanoServiceError, NanoServiceErrorStatus, ResultExt};

#[derive(Deserialize)]
pub struct CreateUser {
    pub email: String,
    pub password: String,
}

pub async fn create<T: SaveOne>(store: &T, data: CreateUser) -> Result<User, NanoServiceError> {
    let email = data.email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return Err(NanoServiceError::new(
            "invalid email",
            NanoServiceErrorStatus::BadRequest,
        ));
    }
    if !(8..=128).contains(&data.password.chars().count()) {
        return Err(NanoServiceError::new(
            "password must be 8-128 chars",
            NanoServiceErrorStatus::BadRequest,
        ));
    }

    let user = tokio::task::spawn_blocking(move || NewUser::new(email, data.password))
        .await
        .eject(NanoServiceErrorStatus::Unknown)??;
    let created_item = store.save_one(user).await?;
    Ok(created_item)
}
