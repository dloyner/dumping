use glue::errors::NanoServiceError;
use to_do_dal::to_do_items::schema::UpdateToDoStatus;
use to_do_dal::to_do_items::transactions::update::UpdateOne;

pub async fn update<T: UpdateOne>(
    store: &T,
    item: UpdateToDoStatus,
    user_id: &str,
) -> Result<(), NanoServiceError> {
    store.update_one(item, user_id).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::update;
    use glue::errors::{NanoServiceError, NanoServiceErrorStatus};
    use to_do_dal::to_do_items::{
        enums::TaskStatus,
        schema::{ToDoItem, UpdateToDoStatus},
        transactions::update::UpdateOne,
    };

    struct Store;
    impl UpdateOne for Store {
        async fn update_one(
            &self,
            item: UpdateToDoStatus,
            user_id: &str,
        ) -> Result<ToDoItem, NanoServiceError> {
            if user_id == "user-1" && item.id == 5 {
                Ok(ToDoItem {
                    id: item.id,
                    title: "updated".into(),
                    status: item.status,
                })
            } else {
                Err(NanoServiceError::new(
                    "Item not found",
                    NanoServiceErrorStatus::NotFound,
                ))
            }
        }
    }

    #[tokio::test]
    async fn update_succeeds_and_preserves_store_errors() {
        let item = UpdateToDoStatus {
            id: 5,
            status: TaskStatus::Done,
        };
        assert!(update(&Store, item.clone(), "user-1").await.is_ok());
        let error = update(&Store, item, "other-user").await.unwrap_err();
        assert_eq!(error.status, NanoServiceErrorStatus::NotFound);
    }
}
