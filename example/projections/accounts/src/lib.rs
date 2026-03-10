use events::OpenedAccount;
use umari_core::{
    EventSet,
    error::ProjectionError,
    event::StoredEvent,
    export_projection,
    prelude::{EventHandler, execute},
    runtime,
};

export_projection!(AccountsProjection);

struct AccountsProjection;

#[derive(EventSet)]
enum Query {
    OpenedAccount(OpenedAccount),
}

impl EventHandler for AccountsProjection {
    type Query = Query;

    fn init() -> Result<Self, ProjectionError> {
        execute(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                account_id TEXT PRIMARY KEY,
                balance INT NOT NULL
            )
            "#,
            vec![],
        )?;

        Ok(AccountsProjection)
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectionError> {
        match event.data {
            Query::OpenedAccount(OpenedAccount {
                account_id,
                initial_balance,
            }) => {
                println!("got opened account event");
                execute(
                    "INSERT INTO accounts (account_id, balance) VALUES (?1, ?2)",
                    vec![
                        runtime::projection::Value::Text(account_id),
                        runtime::projection::Value::Real(initial_balance),
                    ],
                )?;
            }
        }

        Ok(())
    }
}
