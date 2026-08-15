use serde::{Deserialize, Serialize};
use umari::prelude::*;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("Incremented")]
pub struct Incremented {
    #[domain_id]
    pub counter_id: String,
    pub amount: i64,
}

export_effect!(Recorder);

#[derive(EventSet)]
enum Query {
    Incremented(Incremented),
}

struct Recorder {}

impl Effect for Recorder {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "CREATE TABLE IF NOT EXISTS processed (
                event_id TEXT PRIMARY KEY,
                counter_id TEXT NOT NULL,
                amount INTEGER NOT NULL
            )",
        )?;
        Ok(Recorder {})
    }

    fn partition_key(&self, event: StoredEvent<Self::Query>) -> Option<String> {
        match event.data {
            Query::Incremented(ev) => Some(ev.counter_id),
        }
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        let event_id = event.id.to_string();
        match event.data {
            Query::Incremented(ev) => {
                // INSERT OR IGNORE keeps the side effect idempotent under at-least-once redelivery.
                execute(
                    "INSERT OR IGNORE INTO processed (event_id, counter_id, amount) VALUES (?1, ?2, ?3)",
                    params![event_id, ev.counter_id, ev.amount],
                )?;
            }
        }
        Ok(())
    }
}
