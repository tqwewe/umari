# Appendix B: Migration from Traditional Event Sourcing

If you're coming from a traditional event sourcing background (EventStoreDB, Marten, Axon, etc.), this chapter maps familiar concepts to Umari equivalents.

## Key differences

| Concept | Traditional ES | Umari |
|---------|---------------|-------|
| **Event streams** | Per-aggregate streams | Single global log with domain ID tags |
| **Concurrency** | Stream position check | DCB — dynamic boundaries by event overlap |
| **Aggregates** | Aggregate root with state | No aggregates; folds derive state on-demand |
| **Read models** | External projections | Projectors (WASM modules with SQLite) |
| **Side effects** | Process managers / sagas | Effects (WASM modules with HTTP access) |
| **Business logic** | In-process | WASM components, hot-reloadable |
| **State derivation** | Aggregate replay | Folds (event → state reducer) |
| **Idempotency** | Idempotency keys on commands | Built-in via idempotency_key + domain checks |

## Aggregate → Fold + Command

Traditional:

```csharp
public class Widget : Aggregate
{
    public WidgetId Id { get; private set; }
    public string Name { get; private set; }
    public bool Archived { get; private set; }

    public void Create(CreateWidget command)
    {
        if (Version > 0) throw new Exception("Already exists");
        Apply(new WidgetCreated(command.WidgetId, command.Name));
    }

    public void When(WidgetCreated e)
    {
        Id = e.WidgetId;
        Name = e.Name;
    }
}
```

Umari:

```rust
// State — just data
#[derive(Default)]
pub struct WidgetState {
    pub exists: bool,
    pub name: Option<String>,
    pub archived: bool,
}

// Fold — binds to domain IDs, replays events
#[derive(DomainIds, FromDomainIds)]
pub struct WidgetFold {
    #[domain_id]
    pub widget_id: Uuid,
}

#[derive(EventSet)]
pub enum WidgetEvents {
    #[scope(widget_id)]
    WidgetCreated(WidgetCreated),
    #[scope(widget_id)]
    WidgetArchived(WidgetArchived),
}

impl Fold for WidgetFold {
    type Events = WidgetEvents;
    type State = WidgetState;

    fn apply(&self, state: &mut WidgetState, event: StoredEvent<WidgetEvents>) {
        match event.data {
            WidgetEvents::WidgetCreated(ev) => {
                state.exists = true;
                state.name = Some(ev.name);
            }
            WidgetEvents::WidgetArchived(_) => state.archived = true,
        }
    }
}

// Command — validates, checks invariants, emits events
#[export_command]
pub fn create_widget(input: Input, context: CommandContext) -> anyhow::Result<ExecuteOutput> {
    Command::new(input, context)
        .fold::<WidgetFold>()
        .execute(|input, widget| {
            anyhow::ensure!(!widget.exists, "widget already exists");
            Ok(emit![WidgetCreated {
                widget_id: input.widget_id,
                name: input.name,
            }])
        })
}
```

Key differences:
- The fold struct is separate from the state — the fold holds domain ID bindings, the state holds the derived data
- `apply()` takes `&self` (the fold bindings) and a mutable state reference
- The command function is stateless — no aggregate instance, just pure logic
- Consistency is per-domain-ID, not per-aggregate-stream

## Projection → Projector

Traditional:

```csharp
public class WidgetProjection : Projection<WidgetReadModel>
{
    public WidgetProjection()
    {
        Project<WidgetCreated>(e => Insert(new WidgetReadModel { Id = e.WidgetId, Name = e.Name }));
        Project<WidgetArchived>(e => Update(e.WidgetId, w => w.Archived = true));
    }
}
```

Umari:

```rust
impl Projector for Widgets {
    type Query = WidgetQuery;

    fn init() -> anyhow::Result<Self> {
        execute_batch("CREATE TABLE IF NOT EXISTS widgets (
            widget_id TEXT PRIMARY KEY,
            name TEXT NOT NULL,
            archived BOOLEAN NOT NULL DEFAULT FALSE
        )")?;
        Ok(Widgets {})
    }

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            WidgetQuery::WidgetCreated(ev) => {
                execute("INSERT INTO widgets (widget_id, name) VALUES (?1, ?2)",
                    params![ev.widget_id, ev.name])?;
            }
            WidgetQuery::WidgetArchived(ev) => {
                execute("UPDATE widgets SET archived = TRUE WHERE widget_id = ?1",
                    params![ev.widget_id])?;
            }
        }
        Ok(())
    }
}
```

Key differences:
- Projectors are WASM modules, not in-process projections
- Each projector gets its own SQLite database
- `init()` is called once; `handle()` is called per event
- Projectors are naturally idempotent (replay-safe)

## Process Manager / Saga → Effect

Traditional:

```csharp
public class OrderSaga : Saga<OrderSagaState>,
    IAmStartedBy<OrderPlaced>,
    IHandle<PaymentReceived>
{
    public async Task Handle(OrderPlaced e)
    {
        Data.OrderId = e.OrderId;
        await Bus.Send(new ProcessPayment(e.OrderId, e.Amount));
        MarkAsComplete();
    }
}
```

Umari:

```rust
impl Effect for OrderProcessor {
    type Query = OrderEvents;

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            OrderEvents::OrderPlaced(ev) => {
                // Schedule — check if already processed
                let receipt = SchedulePaymentProcessing::execute(
                    &SchedulePaymentProcessingInput { order_id: ev.order_id },
                )?;
                if !receipt.has_event::<PaymentProcessingScheduled>() {
                    return Ok(());
                }

                // Side effect — call payment gateway
                let response = self.http_client
                    .post("https://payment.example.com/process")
                    .json(&json!({ "order_id": ev.order_id, "amount": ev.amount }))
                    .send()?;

                // Record — persist outcome
                RecordPaymentResult::execute(
                    &RecordPaymentResultInput {
                        order_id: ev.order_id,
                        success: response.status().is_success(),
                    },
                )?;
            }
        }
        Ok(())
    }
}
```

Key differences:
- Effects use the fold-check → side effect → record pattern for idempotency
- Effects have their own SQLite for internal state (like saga state) but it's not the idempotency source
- Effects execute commands directly (no message bus)
- Effects can make HTTP calls directly via WASI

## EventStoreDB streams → UmaDB DCB

Traditional:
```
Stream "widget-abc" → [WidgetCreated, WidgetRenamed, WidgetArchived]
Stream "widget-def" → [WidgetCreated]
```

Umari:
```
Global log:
  pos 1: WidgetCreated { widget_id: "abc", ... }           tags: [widget_id:abc]
  pos 2: WidgetCreated { widget_id: "def", ... }           tags: [widget_id:def]
  pos 3: WidgetRenamed { widget_id: "abc", name: "new" }  tags: [widget_id:abc]
  pos 4: WidgetArchived { widget_id: "abc" }               tags: [widget_id:abc]
```

When command queries `widget_id=abc`: gets events at positions 1, 3, 4. No pre-partitioning needed.

## Common migration patterns

### 1. Start with events

Port your event definitions first:

```rust
// Old: AccountCreated { AccountId, OwnerName }
#[derive(Event, Serialize, Deserialize)]
#[event_type("account.created")]
pub struct AccountCreated {
    #[domain_id]
    pub account_id: String,
    pub owner_name: String,
}
```

Add domain IDs to all events. Choose which fields identify the entity.

### 2. Port aggregates to folds

For each aggregate, create:
- A state struct with `#[derive(Default)]`
- A fold struct with `#[derive(DomainIds, FromDomainIds)]`
- An EventSet enum
- Implement `Fold`

The `apply()` method replaces your aggregate's `When()` handlers.

### 3. Port command handlers

Replace aggregate method calls with the `Command::new().fold().execute()` pattern. Invariant checks go in the execute closure. Event emission uses `emit![]`.

### 4. Port projections to projectors

Port your projection SQL to `init()` and `handle()`. Each projector is a separate crate with its own SQLite database. Use `execute_batch()` for DDL, `execute()`/`params![]` for DML.

### 5. Port sagas to effects

Replace message bus interactions with direct command execution. Replace saga state stored in a database with SQLite (internal state) and the event store (idempotency anchor).
