use events::OpenedAccount;
use umari_core::prelude::*;

export_projector!(AccountsProjector);

struct AccountsProjector {
    insert_account: Statement,
}

impl AccountsProjector {
    fn dump_accounts(&self) -> Result<(), SqliteError> {
        let stmt = prepare("SELECT account_id, balance FROM accounts")?;
        let rows = stmt.query(())?;
        for row in rows {
            let account_id = row.get("account_id").unwrap();
            let balance = row.get("balance").unwrap();
            println!("{account_id:?} :   {balance:?}");
        }
        println!("==================================");

        Ok(())
    }

    fn insert_account(
        &self,
        account_id: String,
        initial_balance: f64,
    ) -> Result<usize, SqliteError> {
        self.insert_account.execute((account_id, initial_balance))
    }
}

#[derive(EventSet)]
enum Query {
    OpenedAccount(OpenedAccount),
}

impl Projector for AccountsProjector {
    type Query = Query;

    fn init() -> Result<Self, ProjectorError> {
        execute(
            r#"
            CREATE TABLE IF NOT EXISTS accounts (
                account_id TEXT PRIMARY KEY,
                balance REAL NOT NULL
            )
            "#,
            (),
        )?;

        let projector = AccountsProjector {
            insert_account: prepare("INSERT INTO accounts (account_id, balance) VALUES (?1, ?2)")?,
        };

        projector.dump_accounts()?;

        Ok(projector)
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> Result<(), ProjectorError> {
        match event.data {
            Query::OpenedAccount(OpenedAccount {
                account_id,
                initial_balance,
            }) => {
                self.insert_account(account_id, initial_balance)?;
            }
        }

        self.dump_accounts()?;

        Ok(())
    }
}
