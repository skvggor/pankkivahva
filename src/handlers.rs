use axum::{
    body::Body,
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Json, Response},
};

use chrono::{DateTime, SecondsFormat::Micros, Utc};

use sqlx::{PgPool, Row};

use crate::messages::*;
use crate::models::*;

const MIN_DESCRICAO_LENGTH: usize = 10;
const MIN_VALOR: i32 = 0;
const TIPO_CREDITO: &str = "c";
const TIPO_DEBITO: &str = "d";

pub async fn get_customer_info(
    customer_id: i32,
    db_transaction: &mut sqlx::PgConnection,
) -> Result<Cliente, Response<Body>> {
    let result = sqlx::query("SELECT id, limite, saldo FROM clientes WHERE id = $1 FOR UPDATE")
        .bind(customer_id)
        .fetch_one(db_transaction)
        .await;

    match result {
        Ok(info) => Ok(Cliente {
            id: info.get(0),
            limite: info.get(1),
            saldo: info.get(2),
        }),
        Err(_) => Err((StatusCode::NOT_FOUND, CLIENTE_NAO_ENCONTRADO).into_response()),
    }
}

pub async fn process_transaction(
    request: Json<TransacaoRequest>,
    customer_id: i32,
    pg_pool: PgPool,
) -> Response<Body> {
    if request.valor <= MIN_VALOR {
        return (StatusCode::UNPROCESSABLE_ENTITY, VALOR_INVALIDO).into_response();
    }

    if request.tipo != TIPO_CREDITO && request.tipo != TIPO_DEBITO {
        return (StatusCode::UNPROCESSABLE_ENTITY, TIPO_DE_TRANSACAO_INVALIDA).into_response();
    }

    if request.descricao.is_empty() || request.descricao.len() > MIN_DESCRICAO_LENGTH {
        return (StatusCode::UNPROCESSABLE_ENTITY, DESCRICAO_INVALIDA).into_response();
    }

    let mut db_transaction = match pg_pool.begin().await {
        Ok(current_transaction) => current_transaction,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_INICIAR_TRANSACAO).into_response()
        }
    };

    let customer_info: Cliente = match get_customer_info(customer_id, &mut db_transaction).await {
        Ok(info) => info,
        Err(_) => {
            let _ = db_transaction.rollback().await;

            return (StatusCode::NOT_FOUND, CLIENTE_NAO_ENCONTRADO).into_response();
        }
    };

    if customer_info.id != customer_id {
        let _ = db_transaction.rollback().await;

        return (StatusCode::NOT_FOUND, CLIENTE_NAO_ENCONTRADO).into_response();
    }

    let saldo: i32 = if request.tipo == TIPO_DEBITO {
        customer_info.saldo - request.valor
    } else {
        customer_info.saldo + request.valor
    };

    if request.tipo == TIPO_DEBITO && saldo < -customer_info.limite {
        let _ = db_transaction.rollback().await;

        return (StatusCode::UNPROCESSABLE_ENTITY, SALDO_INSUFICIENTE).into_response();
    }

    let update_result = sqlx::query("UPDATE clientes SET saldo = $1 WHERE id = $2")
        .bind(saldo)
        .bind(customer_id)
        .execute(&mut *db_transaction)
        .await;

    if (update_result).is_err() {
        let _ = db_transaction.rollback().await;

        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            ERRO_ATUALIZAR_SALDO_CLIENTE,
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

        return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_INSERIR_TRANSACAO).into_response();
    }

    if (db_transaction.commit().await).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_FINALIZAR_TRANSACAO).into_response();
    }

    let response = TransacaoResponse {
        limite: customer_info.limite,
        saldo,
    };

    Json(response).into_response()
}

pub async fn handler_transaction(
    Path(customer_id): Path<i32>,
    State(pg_pool): State<PgPool>,
    body: Option<Json<TransacaoRequest>>,
) -> impl IntoResponse {
    match body {
        Some(request) => process_transaction(request, customer_id, pg_pool).await,
        None => (StatusCode::BAD_REQUEST, CORPO_INVALIDO).into_response(),
    }
}

pub async fn handler_account_statement(
    Path(customer_id): Path<i32>,
    State(pg_pool): State<PgPool>,
) -> impl IntoResponse {
    let mut db_transaction = match pg_pool.begin().await {
        Ok(current_transaction) => current_transaction,
        Err(_) => {
            return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_INICIAR_TRANSACAO).into_response()
        }
    };

    let transactions = match sqlx::query("SELECT valor, tipo, descricao, realizada_em FROM transacoes WHERE id_cliente = $1 ORDER BY realizada_em DESC LIMIT 10 FOR UPDATE")
        .bind(customer_id)
        .fetch_all(&mut *db_transaction)
        .await {
            Ok(result) => result,

            Err(_) => {
                return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_BUSCAR_EXTRATO).into_response();
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

    let cliente: Cliente =
        match sqlx::query("SELECT saldo, limite FROM clientes WHERE id = $1 FOR UPDATE")
            .bind(customer_id)
            .fetch_one(&mut *db_transaction)
            .await
        {
            Ok(info) => Cliente {
                id: customer_id,
                saldo: info.get(0),
                limite: info.get(1),
            },
            Err(_) => {
                return (StatusCode::NOT_FOUND, CLIENTE_NAO_ENCONTRADO).into_response();
            }
        };

    let response = ExtratoResponse {
        saldo: Saldo {
            total: cliente.saldo,
            data_extrato: Utc::now().to_rfc3339_opts(Micros, true).to_string(),
            limite: cliente.limite,
        },
        ultimas_transacoes: transacoes,
    };

    if (db_transaction.commit().await).is_err() {
        return (StatusCode::INTERNAL_SERVER_ERROR, ERRO_FINALIZAR_TRANSACAO).into_response();
    }

    Json(response).into_response()
}
