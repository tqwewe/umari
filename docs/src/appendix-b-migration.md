# Migration from Traditional Event Sourcing

If you're coming from a traditional event sourcing background (EventStoreDB, Marten, Axon, etc.), this chapter maps familiar concepts to Umari equivalents.

## Key differences

| Concept | Traditional ES | Umari |
|---------|---------------|-------|
| **Event streams** | Per-aggregate streams | Single global log with domain ID tags |
| **Concurrency** | Stream position check | DCB: dynamic boundaries by event overlap |
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

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// State: just data
#[derive(Default)]
pub struct WidgetState {
    pub exists: bool,
    pub name: Option<String>,
    pub archived: bool,
}

// Fold: binds to domain IDs, replays events
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

// Command: validates, checks invariants, emits events
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

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// Fold: binds to domain IDs, replays events into a mutable state object
export const WidgetFold = defineFold({
  domainIds: ["widgetId"] as const,
  events: [WidgetCreated, WidgetArchived],
  initial: () => ({ exists: false, name: undefined as string | undefined, archived: false }),
  apply: (state, event) => {
    switch (event.type) {
      case "widget.created":
        state.exists = true;
        state.name = event.data.name;
        break;
      case "widget.archived":
        state.archived = true;
        break;
    }
  },
});

// Command: validates, checks invariants, emits events
const CreateWidget = defineCommand<Input, { widget: ReturnType<typeof WidgetFold> }>({
  domainIds: ["widgetId"] as const,
  folds: ({ widgetId }) => ({ widget: WidgetFold({ widgetId }) }),
  execute: ({ input, folds, emit, reject }) => {
    if (folds.widget.exists) reject("widget already exists");
    return emit(WidgetCreated({ widgetId: input.widgetId, name: input.name }));
  },
});

export const { schema, execute } = exportCommand(CreateWidget);
```

{{#endtab }}
{{#endtabs }}

Key differences:
- The derived state is separate from the binding: the fold holds the domain ID bindings, the state holds the derived data
- `apply` updates a mutable state in place (in Rust via `&mut`, in TypeScript by mutating the state object)
- The command is stateless: no aggregate instance, just pure logic
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

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
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

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
const Widgets = defineProjector({
  events: [WidgetCreated, WidgetArchived],
  init: () => {
    sqlite.executeBatch(`
      CREATE TABLE IF NOT EXISTS widgets (
        widget_id TEXT PRIMARY KEY,
        name TEXT NOT NULL,
        archived INTEGER NOT NULL DEFAULT 0
      )
    `);
  },
  handle: (event) => {
    switch (event.type) {
      case "widget.created":
        sqlite.execute("INSERT INTO widgets (widget_id, name) VALUES (?, ?)",
          [event.data.widgetId, event.data.name]);
        break;
      case "widget.archived":
        sqlite.execute("UPDATE widgets SET archived = 1 WHERE widget_id = ?",
          [event.data.widgetId]);
        break;
    }
  },
});

export const { projector } = exportProjector(Widgets);
```

{{#endtab }}
{{#endtabs }}

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

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
impl Effect for OrderProcessor {
    type Query = OrderEvents;

    fn handle(&mut self, event: StoredEvent<Self::Query>) -> anyhow::Result<()> {
        match event.data {
            OrderEvents::OrderPlaced(ev) => {
                // 1. Fold-check via a private command: if the "scheduled"
                //    event was already emitted, the command short-circuits
                //    and the receipt is empty.
                let receipt = schedule_payment_processing(
                    SchedulePaymentProcessingInput { order_id: ev.order_id },
                    CommandContext::new(),
                )?;
                if !receipt.has_event::<PaymentProcessingScheduled>() {
                    return Ok(()); // already processed on a previous run
                }

                // 2. Side effect: call the payment gateway.
                let response = self.http_client
                    .post("https://payment.example.com/process")
                    .json(&json!({ "order_id": ev.order_id, "amount": ev.amount }))
                    .send()?;

                // 3. Record outcome via another private command.
                record_payment_result(
                    RecordPaymentResultInput {
                        order_id: ev.order_id,
                        success: response.status().is_success(),
                    },
                    CommandContext::new(),
                )?;
            }
        }
        Ok(())
    }
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
const OrderProcessor = defineEffect({
  events: [OrderPlaced],
  init: () => ({}),
  partitionKey: (event) => event.data.orderId,
  handle: async (event) => {
    const { orderId, amount } = event.data;

    // 1. Fold-check: has payment already been processed for this order?
    const { processed } = foldQuery({
      processed: EventFold(PaymentProcessed)({ orderId }),
    }).run();
    if (processed.length > 0) return; // already processed on a previous run

    // 2. Side effect: call the payment gateway.
    const res = await fetch("https://payment.example.com/process", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ orderId, amount }),
    });

    // 3. Record outcome via a private command.
    execute("record-payment-result", { orderId, success: res.ok }, {
      correlationId: event.correlationId,
      triggeringEventId: event.id,
      idempotencyKey: event.id,
    });
  },
});

export const { effect } = exportEffect(OrderProcessor);
```

{{#endtab }}
{{#endtabs }}

Key differences:
- Effects use the fold-check → side effect → record pattern for idempotency.
- Effects have their own SQLite for internal state, but it's not the idempotency source: the event store is.
- Effects call commands directly (a function call in Rust; `execute(name, …)` in TypeScript); no message bus.
- HTTP is provided via WASI (`wasi-http-client` in Rust, `fetch` in TypeScript); no host-side bridging code.

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

Port your event definitions first, adding domain IDs to identify the entity each event is about.

{{#tabs global="lang" }}
{{#tab name="Rust" }}

```rust,noplayground
// Old: AccountCreated { AccountId, OwnerName }
#[derive(Event, DomainIds, Serialize, Deserialize)]
#[event_type("account.created")]
pub struct AccountCreated {
    #[domain_id]
    pub account_id: String,
    pub owner_name: String,
}
```

{{#endtab }}
{{#tab name="TypeScript" }}

```ts
// Old: AccountCreated { AccountId, OwnerName }
type AccountCreatedData = { accountId: string; ownerName: string };

export const AccountCreated = defineEvent<AccountCreatedData>()("account.created", {
  domainIds: ["accountId"],
});
```

{{#endtab }}
{{#endtabs }}

### 2. Port aggregates to folds

For each aggregate, define a fold: the derived state shape, the events it reads, and an `apply` that updates the state. The `apply` body replaces your aggregate's `When()` handlers. (Rust: a `Fold` impl with state + `EventSet` enum; TypeScript: `defineFold({ domainIds, events, initial, apply })`.)

### 3. Port command handlers

Replace aggregate method calls with a command. Invariant checks go in the body; event emission uses `emit`. (Rust: `Command::new().fold().execute()`; TypeScript: `defineCommand({ folds, execute })` + `exportCommand`.)

### 4. Port projections to projectors

Port your projection SQL to `init` (DDL) and `handle` (per-event DML). Each projector is its own module with its own SQLite database. (Rust: `execute_batch`/`execute` + `params!`; TypeScript: `sqlite.executeBatch`/`sqlite.execute` with a params array.)

### 5. Port sagas to effects

Replace message-bus interactions with direct command execution, and saga state in a database with SQLite (internal state) plus the event store (idempotency anchor). (Rust: call the private command function; TypeScript: `execute(name, input, ctx)`.)
