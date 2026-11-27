//! CRUD vs REST
//!
//! CRUD = WHAT you do with data (Create, Read, Update, Delete), it maps to SQL
//! INSERT / SELECT / UPDATE / DELETE.
//! REST = HOW you design an HTTP API: resources as nouns in URLs, proper methods
//! and status codes, stateless requests, JSON representations.
//! A REST API is usually built around CRUD, but CRUD can be done without REST
//! (SQL, gRPC, `POST /createUser`), and REST also has non-CRUD actions
//! (`POST /orders/42/cancel`).
//!
//!   POST   /users      -> create   201 Created
//!   GET    /users      -> list     200 OK
//!   GET    /users/{id} -> read     200 OK / 404 Not Found
//!   PUT    /users/{id} -> update   200 OK / 404 Not Found
//!   DELETE /users/{id} -> delete   204 No Content / 404 Not Found

use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

use axum::{
    Json, Router,
    extract::{Path, State},
    http::StatusCode,
    routing::{get, post},
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize)]
struct User {
    id: u64,
    name: String,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
}

#[derive(Default)]
struct Inner {
    next_id: u64,
    users: BTreeMap<u64, User>,
}

#[derive(Clone, Default)]
struct Db(Arc<Mutex<Inner>>);

// Create
async fn create(State(db): State<Db>, Json(body): Json<NewUser>) -> (StatusCode, Json<User>) {
    let mut g = db.0.lock().expect("poisoned");
    g.next_id += 1;
    let user = User { id: g.next_id, name: body.name };
    g.users.insert(user.id, user.clone());
    (StatusCode::CREATED, Json(user))
}

// Read (collection)
async fn list(State(db): State<Db>) -> Json<Vec<User>> {
    Json(db.0.lock().expect("poisoned").users.values().cloned().collect())
}

// Read (one)
async fn read(State(db): State<Db>, Path(id): Path<u64>) -> Result<Json<User>, StatusCode> {
    let g = db.0.lock().expect("poisoned");
    g.users.get(&id).cloned().map(Json).ok_or(StatusCode::NOT_FOUND)
}

// Update
async fn update(
    State(db): State<Db>,
    Path(id): Path<u64>,
    Json(body): Json<NewUser>,
) -> Result<Json<User>, StatusCode> {
    let mut g = db.0.lock().expect("poisoned");
    let user = g.users.get_mut(&id).ok_or(StatusCode::NOT_FOUND)?;
    user.name = body.name;
    Ok(Json(user.clone()))
}

// Delete
async fn remove(State(db): State<Db>, Path(id): Path<u64>) -> StatusCode {
    match db.0.lock().expect("poisoned").users.remove(&id) {
        Some(_) => StatusCode::NO_CONTENT,
        None => StatusCode::NOT_FOUND,
    }
}

pub fn router() -> Router {
    Router::new()
        .route("/users", post(create).get(list))
        .route("/users/{id}", get(read).put(update).delete(remove))
        .with_state(Db::default())
}
