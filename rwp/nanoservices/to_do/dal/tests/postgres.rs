#![cfg(feature = "sqlx-postgres")]

use sqlx::PgPool;
use to_do_dal::to_do_items::{
    descriptors::SqlxPostgresDescriptor, enums::TaskStatus, schema::NewToDoItem,
    transactions::create::SaveOne,
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
