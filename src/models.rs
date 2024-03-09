use serde::{Deserialize, Serialize};
use sqlx::prelude::FromRow;

#[derive(Serialize, Deserialize, FromRow)]
pub struct Cliente {
    pub id: i32,
    pub limite: i32,
    pub saldo: i32,
}

#[derive(Serialize, Deserialize)]
pub struct Saldo {
    pub total: i32,
    pub data_extrato: String,
    pub limite: i32,
}

#[derive(Serialize, Deserialize)]
pub struct TransacaoRequest {
    pub valor: i32,
    pub tipo: String,
    pub descricao: String,
}

#[derive(Serialize, Deserialize, FromRow)]
pub struct Transacao {
    pub valor: i32,
    pub tipo: String,
    pub descricao: String,
    pub realizada_em: String,
}

#[derive(Serialize, Deserialize)]
pub struct TransacaoResponse {
    pub limite: i32,
    pub saldo: i32,
}

#[derive(Serialize, Deserialize)]
pub struct ExtratoResponse {
    pub saldo: Saldo,
    pub ultimas_transacoes: Vec<Transacao>,
}
