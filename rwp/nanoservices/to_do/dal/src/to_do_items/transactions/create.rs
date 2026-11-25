use std::future::Future;

use crate::to_do_items::schema::{NewToDoItem, ToDoItem};
use glue::{db::db_error, errors::NanoServiceError};

#[cfg(feature = "sqlx-postgres")]
use super::super::descriptors::SqlxPostgresDescriptor;

pub trait SaveOne {
    fn save_one(
        &self,
        item: NewToDoItem,
        user_id: &str,
    ) -> impl Future<Output = Result<ToDoItem, NanoServiceError>> + Send;
}

#[cfg(feature = "sqlx-postgres")]
impl SaveOne for SqlxPostgresDescriptor {
    async fn save_one(
        &self,
        item: NewToDoItem,
        user_id: &str,
    ) -> Result<ToDoItem, NanoServiceError> {
        sqlx::query_as::<_, ToDoItem>(
            "INSERT INTO to_do_items (title, status, user_id) VALUES ($1, $2, $3) \
             RETURNING id, title, status",
        )
        .bind(item.title)
        .bind(item.status.to_string())
        .bind(user_id)
        .fetch_one(&self.pool)
        .await
        .map_err(db_error)
    }
}
