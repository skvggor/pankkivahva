mod db;
mod handlers;
mod models;

use crate::handlers::*;

use axum::{
    extract::Path,
    http::Error,
    routing::{get, post},
    Router,
};

use std::result::Result;

#[tokio::main]
async fn main() -> Result<(), Error> {
    let pg_pool = db::connect().await.unwrap();

    let port = std::env::var("PORT").unwrap_or_else(|_| "3000".to_string());

    let app = Router::new()
        .route("/", get(|| async { "OK" }))
        .route(
            "/clientes/:customer_id/extrato",
            get(handler_account_statement),
        )
        .route(
            "/clientes/:customer_id/transacoes",
            post(handler_transaction),
        )
        .with_state(pg_pool);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:".to_string() + &port)
        .await
        .unwrap();

    axum::serve(listener, app).await.unwrap();

    println!("Server running on port {}", port);

    Ok(())
}
