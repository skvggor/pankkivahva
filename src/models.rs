use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
pub struct Cliente {
    pub id: i32,
    pub limite: i32,
    pub saldo: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Saldo {
    pub total: i32,
    pub data_extrato: String,
    pub limite: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransacaoRequest {
    pub valor: i32,
    pub tipo: String,
    pub descricao: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct Transacao {
    pub valor: i32,
    pub tipo: String,
    pub descricao: String,
    pub realizada_em: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct TransacaoResponse {
    pub limite: i32,
    pub saldo: i32,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ExtratoResponse {
    pub saldo: Saldo,
    pub ultimas_transacoes: Vec<Transacao>,
}