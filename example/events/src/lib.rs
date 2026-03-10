use umari_core::prelude::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Event, Serialize, Deserialize)]
pub struct OpenedAccount {
    #[domain_id]
    pub account_id: String,
    pub initial_balance: f64,
}

#[derive(Clone, Debug, PartialEq, Event, Serialize, Deserialize)]
pub struct SentFunds {
    #[domain_id]
    pub account_id: String,
    pub amount: f64,
    pub recipient_id: String,
}

#[derive(Clone, Debug, PartialEq, Event, Serialize, Deserialize)]
pub struct ReceivedFunds {
    #[domain_id]
    pub account_id: String,
    pub amount: f64,
    pub sender_id: String,
}
