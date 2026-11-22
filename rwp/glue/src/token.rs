use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};

use crate::errors::{NanoServiceError, NanoServiceErrorStatus, ResultExt};

const TOKEN_TTL_SECS: u64 = 24 * 60 * 60;
static JWT_SECRET: OnceLock<String> = OnceLock::new();

#[derive(Serialize, Deserialize)]
struct Claims {
    unique_id: String,
    exp: u64,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct HeaderToken {
    pub unique_id: String,
}

impl HeaderToken {
    pub fn validate_secret() -> Result<(), NanoServiceError> {
        let key = Self::get_key()?;
        if key.len() < 32 || key.starts_with("replace with") {
            return Err(NanoServiceError::new(
                "JWT_SECRET too weak",
                NanoServiceErrorStatus::Unknown,
            ));
        }
        Ok(())
    }

    pub fn get_key() -> Result<String, NanoServiceError> {
        if let Some(secret) = JWT_SECRET.get() {
            return Ok(secret.clone());
        }
        let secret = std::env::var("JWT_SECRET").eject(NanoServiceErrorStatus::Unknown)?;
        Ok(JWT_SECRET.get_or_init(|| secret).clone())
    }

    pub fn encode(self) -> Result<String, NanoServiceError> {
        let key = EncodingKey::from_secret(Self::get_key()?.as_bytes());
        let exp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .eject(NanoServiceErrorStatus::Unknown)?
            .as_secs()
            + TOKEN_TTL_SECS;
        let claims = Claims {
            unique_id: self.unique_id,
            exp,
        };
        jsonwebtoken::encode(&Header::default(), &claims, &key)
            .eject(NanoServiceErrorStatus::Unknown)
    }

    pub fn decode(token: &str) -> Result<Self, NanoServiceError> {
        let key = DecodingKey::from_secret(Self::get_key()?.as_bytes());
        jsonwebtoken::decode::<Claims>(token, &key, &Validation::new(Algorithm::HS256))
            .map(|data| HeaderToken {
                unique_id: data.claims.unique_id,
            })
            .eject(NanoServiceErrorStatus::Unauthorized)
    }
}

#[cfg(feature = "axum")]
mod axum_impl {
    use super::HeaderToken;
    use crate::errors::{NanoServiceError, NanoServiceErrorStatus};
    use axum::{extract::FromRequestParts, http::request::Parts};

    impl<S> FromRequestParts<S> for HeaderToken
    where
        S: Send + Sync,
    {
        type Rejection = NanoServiceError;

        async fn from_request_parts(
            parts: &mut Parts,
            _state: &S,
        ) -> Result<Self, Self::Rejection> {
            let Some(raw_data) = parts.headers.get(axum::http::header::AUTHORIZATION) else {
                return Err(NanoServiceError::new(
                    "Authorization header not found",
                    NanoServiceErrorStatus::Unauthorized,
                ));
            };

            let Ok(token) = raw_data.to_str() else {
                return Err(NanoServiceError::new(
                    "Token is not a valid string",
                    NanoServiceErrorStatus::Unauthorized,
                ));
            };

            let token = token
                .split_once(' ')
                .filter(|(scheme, _)| scheme.eq_ignore_ascii_case("bearer"))
                .map(|(_, t)| t.trim())
                .ok_or_else(|| {
                    NanoServiceError::new(
                        "Invalid authorization scheme",
                        NanoServiceErrorStatus::Unauthorized,
                    )
                })?;

            HeaderToken::decode(token)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::HeaderToken;
    use crate::errors::NanoServiceErrorStatus;

    fn configure_secret() {
        static INIT: std::sync::Once = std::sync::Once::new();
        INIT.call_once(|| unsafe { std::env::set_var("JWT_SECRET", "glue-token-tests") });
    }

    #[test]
    fn token_round_trip_preserves_unique_id() {
        configure_secret();
        let encoded = HeaderToken {
            unique_id: "user-42".into(),
        }
        .encode()
        .unwrap();
        let decoded = HeaderToken::decode(&encoded).unwrap();
        assert_eq!(decoded.unique_id, "user-42");
    }

    #[test]
    fn rejects_tampered_tokens_as_unauthorized() {
        configure_secret();
        let encoded = HeaderToken {
            unique_id: "user-42".into(),
        }
        .encode()
        .unwrap();
        let tampered = format!("{encoded}x");
        let error = HeaderToken::decode(&tampered).unwrap_err();
        assert_eq!(error.status, NanoServiceErrorStatus::Unauthorized);
    }
}
