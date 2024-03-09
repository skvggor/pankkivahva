use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

use chrono::{DateTime, SecondsFormat::Micros, Utc};

use sqlx::PgPool;

use crate::models::*;

const MIN_DESCRICAO_LENGTH: usize = 10;
const MIN_VALOR: i32 = 0;
const TIPO_CREDITO: &str = "c";
const TIPO_DEBITO: &str = "d";

pub async fn get_customer_info(
    customer_id: i32,
    db_transaction: &mut sqlx::PgConnection,
) -> Result<Cliente, Response<Body>> {
    let result = sqlx::query_as::<_, Cliente>(
        "SELECT id, limite, saldo FROM clientes WHERE id = $1 FOR UPDATE",
    )
    .bind(customer_id)
    .fetch_one(db_transaction)
    .await
    .map_err(|error| (StatusCode::INTERNAL_SERVER_ERROR, error.to_string()).into_response())?;

    Ok(result)
}

pub async fn process_transaction(
    request: Json<TransacaoRequest>,
    customer_id: i32,
    pg_pool: PgPool,
) -> Response<Body> {
    if request.valor <= MIN_VALOR {
        return (StatusCode::UNPROCESSABLE_ENTITY, "Valor inválido").into_response();
    }

    if request.tipo != TIPO_CREDITO && request.tipo != TIPO_DEBITO {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Tipo de transação inválida",
        )
            .into_response();
    }

    if request.descricao.is_empty() || request.descricao.len() > MIN_DESCRICAO_LENGTH {
        return (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Descrição inválida! Deve possuir 1 à 10 caracteres",
        )
            .into_response();
    }

    let mut db_transaction = match pg_pool.begin().await {
        Ok(current_transaction) => current_transaction,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erro ao iniciar transação",
            )
                .into_response()
        }
    };

    let customer_info: Cliente = match get_customer_info(customer_id, &mut db_transaction).await {
        Ok(info) => info,
        Err(_) => {
            let _ = db_transaction.rollback().await;

            return (StatusCode::NOT_FOUND, "Cliente não encontrado").into_response();
        }
    };

    if customer_info.id != customer_id {
        let _ = db_transaction.rollback().await;
        return (StatusCode::NOT_FOUND, "Cliente não encontrado").into_response();
    }

    let novo_saldo: i32 = if request.tipo == TIPO_DEBITO {
        let saldo_temp = customer_info.saldo - request.valor;

        if saldo_temp < -customer_info.limite {
            let _ = db_transaction.rollback().await;
            return (
                StatusCode::UNPROCESSABLE_ENTITY,
                "Saldo insuficiente para realizar transação",
            )
                .into_response();
        }

        saldo_temp
    } else {
        customer_info.saldo + request.valor
    };

    let update_result = sqlx::query("UPDATE clientes SET saldo = $1 WHERE id = $2")
        .bind(novo_saldo)
        .bind(customer_id)
        .execute(&mut *db_transaction)
        .await;

    if (update_result).is_err() {
        let _ = db_transaction.rollback().await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erro ao atualizar saldo do cliente",
        )
            .into_response();
    }

    let insert_result = sqlx::query("INSERT INTO transacoes (id_cliente, valor, tipo, descricao, realizada_em) VALUES ($1, $2, $3, $4, TO_TIMESTAMP($5, 'YYYY-MM-DD\"T\"HH24:MI:SS.US\"Z\"'))")
        .bind(customer_id)
        .bind(request.valor)
        .bind(&request.tipo)
        .bind(&request.descricao)
        .bind(Utc::now().to_rfc3339_opts(Micros, true))
        .execute(&mut *db_transaction)
        .await;

    if (insert_result).is_err() {
        let _ = db_transaction.rollback().await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erro ao inserir transação",
        )
            .into_response();
    }

    if (db_transaction.commit().await).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erro ao finalizar transação",
        )
            .into_response();
    }

    let response = TransacaoResponse {
        limite: customer_info.limite,
        saldo: novo_saldo,
    };

    Json(response).into_response()
}

pub async fn handler_transaction(
    Path(customer_id): Path<i32>,
    State(pg_pool): State<PgPool>,
    body: Option<Json<TransacaoRequest>>,
) -> impl IntoResponse {
    match body {
        Some(transacao_request) => {
            process_transaction(transacao_request, customer_id, pg_pool).await
        }
        None => (
            StatusCode::UNPROCESSABLE_ENTITY,
            "Corpo da requisição inválido",
        )
            .into_response(),
    };
}

pub async fn handler_account_statement(
    Path(customer_id): Path<i32>,
    State(pg_pool): State<PgPool>,
) -> impl IntoResponse {
    let mut db_transaction = match pg_pool.begin().await {
        Ok(current_transaction) => current_transaction,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                "Erro ao iniciar transação",
            )
                .into_response()
        }
    };

    let transactions: Vec<Transacao> = match sqlx::query_as::<_, Transacao>("SELECT valor, tipo, descricao, realizada_em FROM transacoes WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10 FOR UPDATE")
        .bind(customer_id)
        .fetch_all(&mut *db_transaction)
        .await {
            Ok(result) => result,
            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, "Erro ao buscar transações").into_response();
            }
        };

    let mut transacoes = Vec::new();

    for row in transactions {
        let realizada_em_str: String = row.realizada_em;
        let realizada_em = match DateTime::parse_from_rfc3339(&realizada_em_str) {
            Ok(dt) => dt.with_timezone(&Utc),
            Err(e) => {
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    format!("Erro ao parsear data e hora: {}", e),
                )
                    .into_response();
            }
        };

        transacoes.push(Transacao {
            valor: row.valor,
            tipo: row.tipo,
            descricao: row.descricao,
            realizada_em: realizada_em.to_rfc3339_opts(Micros, true),
        });
    }

    let customer_info: Cliente =
        match sqlx::query_as("SELECT saldo, limite FROM clientes WHERE id = $1 FOR UPDATE")
            .bind(customer_id)
            .fetch_one(&mut *db_transaction)
            .await
        {
            Ok(info) => info,
            Err(_) => {
                let _ = db_transaction.rollback().await;
                return (StatusCode::NOT_FOUND, "Cliente não encontrado").into_response();
            }
        };

    let response = ExtratoResponse {
        saldo: Saldo {
            total: customer_info.saldo,
            data_extrato: Utc::now().to_rfc3339_opts(Micros, true).to_string(),
            limite: customer_info.limite,
        },
        ultimas_transacoes: transacoes,
    };

    if (db_transaction.commit().await).is_err() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            "Erro ao finalizar transação",
        )
            .into_response();
    }

    Json(response).into_response()
}
