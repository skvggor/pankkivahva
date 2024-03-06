use chrono::DateTime;

use sqlx::PgPool;

use axum::body::Body;
use axum::response::Response;
use axum::{extract::Path, http::StatusCode, response::IntoResponse, Json};

use sqlx::Row;

use chrono::Utc;

use serde_json::json;

use crate::models::*;

pub async fn get_customer_info(
    customer_id: i32,
    pg_pool: PgPool,
) -> Result<Cliente, Response<Body>> {
    let result = sqlx::query("SELECT id, limite, saldo FROM clientes WHERE id = $1 FOR UPDATE")
        .bind(customer_id)
        .fetch_one(&pg_pool)
        .await;

    match result {
        Ok(row) => {
            let cliente = Cliente {
                id: row.get(0),
                limite: row.get(1),
                saldo: row.get(2),
            };

            Ok(cliente)
        }

        Err(_) => Err((StatusCode::NOT_FOUND, "Cliente não encontrado").into_response()),
    }
}

pub async fn process_transaction(
    request: Json<TransacaoRequest>,
    customer_id: i32,
    pg_pool: PgPool,
) -> Response<Body> {
    let customer_info: Cliente = get_customer_info(customer_id, pg_pool).await.unwrap();

    if customer_info.id != customer_id {
        return (StatusCode::NOT_FOUND, "Cliente não encontrado").into_response();
    }

    let saldo = customer_info.saldo;
    let limite = customer_info.limite;

    if request.descricao.is_empty() || request.descricao.len() > 10 {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Descrição inválida! Deve possuir 1 à 10 caracteres",
        )
            .into_response();
    }

    if request.valor <= 0 {
        return (StatusCode::UNPROCESSABLE_ENTITY, "Valor inválido").into_response();
    }

    if request.tipo != "c" && request.tipo != "d" {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Tipo de transação inválida",
        )
            .into_response();
    }

    let current_saldo: i32;

    if request.tipo == "d" {
        current_saldo = saldo - request.valor;

        if current_saldo < -limite {
            return (StatusCode::UNPROCESSABLE_ENTITY, "Operação não autorizada").into_response();
        }

        let update_result = sqlx::query("UPDATE clientes SET saldo = $1 WHERE id = $2")
            .bind(current_saldo)
            .bind(customer_id)
            .execute(&pool)
            .await;

        if let Err(_) = update_result {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erro ao atualizar saldo do cliente",
            )
                .into_response();
        }
    } else {
        current_saldo = saldo + request.valor;

        let update_result = sqlx::query("UPDATE clientes SET saldo = $1 WHERE id = $2")
            .bind(current_saldo)
            .bind(customer_id)
            .execute(&pool)
            .await;

        if let Err(_) = update_result {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erro ao atualizar saldo do cliente",
            )
                .into_response();
        }
    }

    let insert_result = sqlx::query("INSERT INTO transacoes (id_cliente, valor, tipo, descricao, realizada_em) VALUES ($1, $2, $3, $4, TO_TIMESTAMP($5, 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'))")
            .bind(customer_id)
            .bind(request.valor)
            .bind(&request.tipo)
            .bind(&request.descricao)
            .bind(Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Micros, true))
            .execute(&pool)
            .await;

    match insert_result {
        Ok(_) => (
            StatusCode::OK,
            Json(json!({
                "saldo": current_saldo,
                "limite": limite

            })),
        )
            .into_response(),

        Err(_) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erro ao realizar transação",
        )
            .into_response(),
    }
}

pub async fn handler_transaction(
    Path(customer_id): Path<i32>,
    body: Option<Json<TransacaoRequest>>,
    pg_pool: PgPool,
) -> impl IntoResponse {
    if let Some(transacao_request) = body {
        process_transaction(transacao_request, customer_id, pg_pool).await
    } else {
        (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Corpo da requisição inválido",
        )
            .into_response()
    }
}

pub async fn handler_account_statement(
    Path(customer_id): Path<i32>,
    pg_pool: PgPool,
) -> impl IntoResponse {
    let transactions = match sqlx::query("SELECT valor, tipo, descricao, realizada_em FROM transacoes WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10")
        .bind(customer_id)
        .fetch_all(&pg_pool)
        .await {
            Ok(result) => result,

            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao buscar transações").into_response();
            }
        };

    let customer_info = match get_customer_info(customer_id, pg_pool).await {
        Ok(info) => info,

        Err(_) => {
            return (StatusCode::NOT_FOUND, "Cliente não encontrado").into_response();
        }
    };

    let mut transacoes = Vec::new();

    for row in transactions {
        let realizada_em: DateTime<Utc> = row.get(3);

        transacoes.push(Transacao {
            valor: row.get(0),
            tipo: row.get(1),
            descricao: row.get(2),
            realizada_em: realizada_em.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        });
    }

    let response = ExtratoResponse {
        saldo: Saldo {
            total: customer_info.saldo,
            data_extrato: Utc::now()
                .to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
                .to_string(),
            limite: customer_info.limite,
        },
        ultimas_transacoes: transacoes,
    };

    return Json(response).into_response();
}
