use serde::{Deserialize, Serialize};
use umari::prelude::*;

#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("Incremented")]
pub struct Incremented {
    #[domain_id]
    pub counter_id: String,
    pub amount: i64,
}

export_projector!(Totals);

#[derive(EventSet)]
enum Query {
    Incremented(Incremented),
}

struct Totals {}

impl Projector for Totals {
    type Query = Query;

    fn init() -> anyhow::Result<Self> {
        execute_batch(
            "CREATE TABLE IF NOT EXISTS totals (counter_id TEXT PRIMARY KEY, total INTEGER NOT NULL DEFAULT 0)",
        )?;
        Ok(Totals {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            Query::Incremented(ev) => {
                execute(
                    "INSERT INTO totals (counter_id, total) VALUES (?1, ?2)
                     ON CONFLICT(counter_id) DO UPDATE SET total = total + excluded.total",
                    params![ev.counter_id, ev.amount],
                )?;
            }
        }
        Ok(())
    }
}
