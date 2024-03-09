use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn connect() -> Result<PgPool, sqlx::Error> {
    let url = "postgres://user:password@localhost:5432/pankkivahva";

    let pool = PgPoolOptions::new()
        .test_before_acquire(false)
        .max_connections(100)
        .connect(url)
        .await?;

    Ok(pool)
}
