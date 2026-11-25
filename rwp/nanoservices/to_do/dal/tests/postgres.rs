#![cfg(feature = "sqlx-postgres")]
use sqlx::PgPool;
use to_do_dal::to_do_items::{
    descriptors::SqlxPostgresDescriptor,
    enums::TaskStatus,
    schema::{NewToDoItem, UpdateToDoStatus},
    transactions::{
        create::SaveOne,
        delete::DeleteOne,
        get::{GetAll, GetById},
        update::UpdateOne,
    },
};

#[sqlx::test(migrations = "./migrations")]
async fn two_users_can_share_a_title(pool: PgPool) {
    let store = SqlxPostgresDescriptor::new(pool);
    let make_item = || NewToDoItem {
        title: "milk".into(),
        status: TaskStatus::Pending,
    };

    store.save_one(make_item(), "1").await.unwrap();
    store.save_one(make_item(), "2").await.unwrap();
}

#[sqlx::test(migrations = "./migrations")]
async fn users_cannot_touch_each_others_items(pool: PgPool) {
    let store = SqlxPostgresDescriptor::new(pool);
    let item = store
        .save_one(
            NewToDoItem {
                title: "private".into(),
                status: TaskStatus::Pending,
            },
            "1",
        )
        .await
        .unwrap();

    assert!(store.get_by_id(item.id, "2").await.is_err());
    assert!(
        store
            .update_one(
                UpdateToDoStatus {
                    id: item.id,
                    status: TaskStatus::Done
                },
                "2"
            )
            .await
            .is_err()
    );
    assert!(store.delete_one(item.id, "2").await.is_err());
    assert!(store.get_all("2").await.unwrap().is_empty());

    store.delete_one(item.id, "1").await.unwrap();
}
