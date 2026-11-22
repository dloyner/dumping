use auth_dal::users::{schema::User, transactions::get::GetByEmail};
use glue::{
    errors::{NanoServiceError, NanoServiceErrorStatus, ResultExt},
    token::HeaderToken,
};

fn invalid_credentials() -> NanoServiceError {
    NanoServiceError::new(
        "invalid email or password",
        NanoServiceErrorStatus::Unauthorized,
    )
}

pub async fn login<T: GetByEmail>(
    store: &T,
    email: &str,
    password: String,
) -> Result<(String, User), NanoServiceError> {
    let email = email.trim().to_lowercase();
    if email.len() > 254 || !email.contains('@') {
        return Err(NanoServiceError::new(
            "invalid email",
            NanoServiceErrorStatus::BadRequest,
        ));
    }

    let user = match store.get_by_email(&email).await {
        Ok(user) => user,
        Err(e) if e.status == NanoServiceErrorStatus::NotFound => {
            tokio::task::spawn_blocking(move || User::verify_dummy_password(&password))
                .await
                .eject(NanoServiceErrorStatus::Unknown)?;
            return Err(invalid_credentials());
        }
        Err(e) => return Err(e),
    };

    let verifier = user.clone();
    let verified = tokio::task::spawn_blocking(move || verifier.verify_password(&password))
        .await
        .eject(NanoServiceErrorStatus::Unknown)??;

    if !verified {
        return Err(invalid_credentials());
    }

    let token = HeaderToken {
        unique_id: user.unique_id.clone(),
    }
    .encode()?;
    Ok((token, user))
}
