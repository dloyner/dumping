use glue::errors::{NanoServiceError, NanoServiceErrorStatus, ResultExt};

use sqlx::PgPool;

pub async fn run_migrations(pool: &PgPool) -> Result<(), NanoServiceError> {
    tracing::info!("Migrating auth database...");

    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .eject(NanoServiceErrorStatus::Unknown)?;

    tracing::info!("auth database migrations completed");
    Ok(())
}
