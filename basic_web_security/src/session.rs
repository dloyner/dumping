//! Session cookie (stateful) vs JWT (stateless), and cookie flags
//!
//! Session cookie: the server stores the state, the browser only holds a random,
//! meaningless ID. Logout/revoke = delete the entry. Needs a shared store to scale.
//! JWT: a signed, self-contained token (payload is only base64, NOT encrypted).
//! No lookup needed, but hard to revoke before `exp`. This module shows the
//! session approach; JWT is described in the README.
//!
//! Where to keep the token in a browser:
//!   - localStorage: readable by JS, so XSS can STEAL the token.
//!   - HttpOnly cookie: JS cannot read it. XSS can still ACT inside the victim's
//!     browser, but cannot take the token away. Pair with Secure + SameSite
//!     (and CSRF protection, since cookies are auto-sent).

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use axum::{
    Router,
    extract::State,
    http::{HeaderMap, StatusCode, header},
    response::IntoResponse,
    routing::{get, post},
};

const COOKIE_NAME: &str = "sid";

/// 256 bits from the OS CSPRNG, hex encoded. Session IDs must be unguessable.
pub fn random_token() -> String {
    let mut buf = [0u8; 32];
    getrandom::fill(&mut buf).expect("OS random source unavailable");
    buf.iter().map(|b| format!("{b:02x}")).collect()
}

/// In-memory session store: session id -> user id.
#[derive(Clone, Default)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<String, u64>>>,
}

impl SessionStore {
    pub fn create(&self, user_id: u64) -> String {
        let id = random_token();
        self.inner.lock().expect("poisoned").insert(id.clone(), user_id);
        id
    }

    pub fn get(&self, id: &str) -> Option<u64> {
        self.inner.lock().expect("poisoned").get(id).copied()
    }

    /// Easy revoke is the main advantage over JWT.
    pub fn revoke(&self, id: &str) -> bool {
        self.inner.lock().expect("poisoned").remove(id).is_some()
    }
}

/// Build a hardened `Set-Cookie` value.
pub fn session_cookie(value: &str, max_age_secs: u64) -> String {
    format!(
        "{COOKIE_NAME}={value}; HttpOnly; Secure; SameSite=Lax; Path=/; Max-Age={max_age_secs}"
    )
}

/// Pick one cookie out of a `Cookie:` request header.
pub fn cookie_value<'a>(header: &'a str, name: &str) -> Option<&'a str> {
    header
        .split(';')
        .filter_map(|kv| kv.trim().split_once('='))
        .find(|(k, _)| *k == name)
        .map(|(_, v)| v)
}

fn sid_from(headers: &HeaderMap) -> Option<&str> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    cookie_value(raw, COOKIE_NAME)
}

async fn login(State(store): State<SessionStore>) -> impl IntoResponse {
    // A real app would verify credentials first. The user id is hardcoded here.
    let sid = store.create(42);
    ([(header::SET_COOKIE, session_cookie(&sid, 7200))], "logged in")
}

async fn me(State(store): State<SessionStore>, headers: HeaderMap) -> Result<String, StatusCode> {
    let sid = sid_from(&headers).ok_or(StatusCode::UNAUTHORIZED)?;
    store
        .get(sid)
        .map(|uid| format!("user {uid}"))
        .ok_or(StatusCode::UNAUTHORIZED)
}

async fn logout(State(store): State<SessionStore>, headers: HeaderMap) -> impl IntoResponse {
    if let Some(sid) = sid_from(&headers) {
        store.revoke(sid);
    }
    // Max-Age=0 tells the browser to drop the cookie.
    ([(header::SET_COOKIE, session_cookie("", 0))], "logged out")
}

pub fn router() -> Router {
    Router::new()
        // State-changing actions use POST, never GET (see csrf).
        .route("/login", post(login))
        .route("/logout", post(logout))
        .route("/me", get(me))
        .with_state(SessionStore::default())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cookie_has_all_security_flags() {
        let c = session_cookie("abc", 60);
        for flag in ["HttpOnly", "Secure", "SameSite=Lax"] {
            assert!(c.contains(flag), "missing {flag}");
        }
    }

    #[test]
    fn session_can_be_revoked() {
        let store = SessionStore::default();
        let sid = store.create(7);
        assert_eq!(store.get(&sid), Some(7));
        assert!(store.revoke(&sid));
        assert_eq!(store.get(&sid), None);
    }

    #[test]
    fn tokens_are_unique_and_long() {
        let (a, b) = (random_token(), random_token());
        assert_ne!(a, b);
        assert_eq!(a.len(), 64);
    }

    #[test]
    fn parses_cookie_header() {
        assert_eq!(cookie_value("a=1; sid=xyz; b=2", "sid"), Some("xyz"));
        assert_eq!(cookie_value("a=1", "sid"), None);
    }
}
