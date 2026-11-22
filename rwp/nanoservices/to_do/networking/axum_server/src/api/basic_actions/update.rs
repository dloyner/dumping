use axum::{Json, extract::State, http::StatusCode, response::IntoResponse};

use auth_kernel::user_session::transactions::get::GetUserSession;
use glue::errors::NanoServiceError;
use to_do_core::api::basic_actions::{get::get_all as get_all_core, update::update as update_core};
use to_do_dal::to_do_items::{
    schema::UpdateToDoStatus, transactions::get::GetAll, transactions::update::UpdateOne,
};

use crate::api::AppState;
use crate::api::basic_actions::auth_user::AuthUser;

pub async fn update<T: UpdateOne + GetAll, X: GetUserSession>(
    State(state): State<AppState<T, X>>,
    AuthUser(owner_key): AuthUser,
    Json(body): Json<UpdateToDoStatus>,
) -> Result<impl IntoResponse, NanoServiceError> {
    update_core(&state.store, body, &owner_key).await?;
    let all_items = get_all_core(&state.store, &owner_key).await?;
    Ok((StatusCode::OK, Json(all_items)))
}
