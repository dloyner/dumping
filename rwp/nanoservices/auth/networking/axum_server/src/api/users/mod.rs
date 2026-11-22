pub mod create;
pub mod get;

use axum::{
    Router,
    routing::{get, post},
};

use auth_dal::users::transactions::AuthStore;

use super::AppState;

pub fn users_router<
    T: AuthStore + Clone + Send + Sync + 'static,
    X: Clone + Send + Sync + 'static,
>() -> Router<AppState<T, X>> {
    Router::new()
        .merge(create_router::<T, X>())
        .merge(get_router::<T, X>())
}

pub fn create_router<
    T: AuthStore + Clone + Send + Sync + 'static,
    X: Clone + Send + Sync + 'static,
>() -> Router<AppState<T, X>> {
    Router::new().nest(
        "/users",
        Router::new().route("/create", post(create::create::<T, X>)),
    )
}

pub fn get_router<
    T: AuthStore + Clone + Send + Sync + 'static,
    X: Clone + Send + Sync + 'static,
>() -> Router<AppState<T, X>> {
    Router::new().nest(
        "/users",
        Router::new().route("/me", get(get::get_me::<T, X>)),
    )
}
