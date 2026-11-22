use glue::errors::{NanoServiceError, NanoServiceErrorStatus, ResultExt};

use sqlx::PgPool;

pub async fn run_migrations(pool: &PgPool) -> Result<(), NanoServiceError> {
    tracing::info!("Migrating to-do database...");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .eject(NanoServiceErrorStatus::Unknown)?;

    tracing::info!("to-do database migrations completed");
    Ok(())
}
