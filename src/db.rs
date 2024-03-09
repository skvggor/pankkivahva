use sqlx::postgres::{PgPool, PgPoolOptions};

pub async fn connect() -> Result<PgPool, sqlx::Error> {
    let url = "postgres://user:password@db:5432/pankkivahva";

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .min_connections(1)
        .connect(url)
        .await?;

    Ok(pool)
}
