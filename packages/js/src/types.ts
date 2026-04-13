// ─── Emitted Event (output of create()) ─────────────────────────────────────

export type EmittedEvent = {
  eventType: string;
  data: string; // JSON-serialized
  domainIds: Array<{ name: string; id: string | null }>;
};

// ─── Stored Event (input from host) ─────────────────────────────────────────

export type StoredEventRaw = {
  id: string;
  position: bigint;
  eventType: string;
  tags: string[];
  timestamp: bigint; // milliseconds since epoch
  correlationId: string;
  causationId: string;
  triggeringEventId?: string;
  idempotencyKey?: string;
  data: string; // JSON string
};

// ─── Typed Event (what apply/handle receive) ────────────────────────────────

export type TypedEvent<TType extends string, TData extends object> = {
  readonly type: TType;
  readonly data: TData;
  readonly id: string;
  readonly position: bigint;
  readonly tags: string[];
  readonly timestamp: number; // ms, converted from bigint
  readonly correlationId: string;
  readonly causationId: string;
  readonly triggeringEventId: string | undefined;
  readonly idempotencyKey: string | undefined;
};

// ─── Event Definition ────────────────────────────────────────────────────────

export type EventDef<
  TType extends string,
  TData extends object,
  TDomainIds extends readonly string[],
> = {
  readonly type: TType;
  readonly domainIds: TDomainIds;
  create(data: TData): EmittedEvent;
};

// ─── Build discriminated union from a tuple of EventDefs ────────────────────

export type EventUnion<TDefs extends readonly EventDef<any, any, any>[]> = {
  [K in keyof TDefs]: TDefs[K] extends EventDef<infer Type, infer Data, any>
    ? TypedEvent<Type, Data>
    : never;
}[number];

// ─── Fold Definition ─────────────────────────────────────────────────────────

export type FoldDef<
  TEvents extends readonly EventDef<any, any, any>[],
  TState,
> = {
  readonly _tag: "fold";
  readonly _events: TEvents;
  readonly _initial: TState | (() => TState);
  readonly _apply: (state: TState, event: EventUnion<TEvents>) => TState;
};

// Extract state type from a FoldDef
export type StateOf<F> = F extends FoldDef<any, infer S> ? S : never;

// For a record of FoldDefs, extract all states
export type StatesOf<T extends Record<string, FoldDef<any, any>>> = {
  readonly [K in keyof T]: StateOf<T[K]>;
};

// ─── Rule Definitions ────────────────────────────────────────────────────────

export type SingleFoldRule<
  TFold extends FoldDef<any, any>,
  TInput extends object,
> = {
  readonly _tag: "rule";
  readonly _kind: "single";
  readonly _fold: TFold;
  readonly _check: (
    state: StateOf<TFold>,
    input: TInput,
  ) => string | null | undefined | void;
};

export type MultiFoldRule<
  TFolds extends Record<string, FoldDef<any, any>>,
  TInput extends object,
> = {
  readonly _tag: "rule";
  readonly _kind: "multi";
  readonly _folds: TFolds;
  readonly _check: (
    states: StatesOf<TFolds>,
    input: TInput,
  ) => string | null | undefined | void;
};

export type RuleDef<TInput extends object = any> =
  | SingleFoldRule<FoldDef<any, any>, TInput>
  | MultiFoldRule<Record<string, FoldDef<any, any>>, TInput>;

// ─── Command Definition ──────────────────────────────────────────────────────

export type CommandDef<
  TInput extends object,
  TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>>,
  TRules extends readonly RuleDef<TInput>[],
> = {
  readonly _tag: "command";
  readonly _schema: { parse(data: unknown): TInput };
  readonly _domainIds: TDomainIds;
  readonly _state: TState;
  readonly _rules: TRules;
  readonly _emit: (states: StatesOf<TState>, input: TInput) => EmittedEvent[];
};

// ─── Event entry for projectors/policies/effects (with optional static scope)

export type EventEntry<E extends EventDef<any, any, any>> =
  | E
  | { event: E; scope: Partial<Record<string, string>> };

export type AnyEventEntry = EventEntry<EventDef<any, any, any>>;

export type EntryEventDef<E extends AnyEventEntry> =
  E extends EventDef<any, any, any>
    ? E
    : E extends { event: infer D }
      ? D
      : never;

export type EventUnionFromEntries<T extends readonly AnyEventEntry[]> = {
  [K in keyof T]: EntryEventDef<T[K]> extends EventDef<
    infer Type,
    infer Data,
    any
  >
    ? TypedEvent<Type, Data>
    : never;
}[number];

// ─── Policy / Effect shared types ────────────────────────────────────────────

export type CommandSubmission = {
  type: string;
  input: object;
};

export type EffectContext = {
  executor: CommandExecutor;
  fetch: (input: string | URL | Request, init?: RequestInit) => Promise<Response>;
};

// ─── SQLite DB Wrapper ───────────────────────────────────────────────────────

export type SqliteRow = Record<string, string | number | bigint | Uint8Array | null>;

export type SqliteDb = {
  execute(sql: string, params?: unknown[]): number;
  executeBatch(sql: string): void;
  queryOne(sql: string, params?: unknown[]): SqliteRow | null;
  queryRow(sql: string, params?: unknown[]): SqliteRow | null;
  query(sql: string, params?: unknown[]): SqliteRow[];
  lastInsertRowId(): bigint;
  prepare(sql: string): SqliteStatement;
};

export type SqliteStatement = {
  execute(params?: unknown[]): number;
  query(params?: unknown[]): SqliteRow[];
  queryOne(params?: unknown[]): SqliteRow | null;
  queryRow(params?: unknown[]): SqliteRow | null;
};

// ─── Command Executor Wrapper ────────────────────────────────────────────────

export type EmittedEventReceipt = {
  id: string;
  eventType: string;
  tags: string[];
};

export type CommandReceipt = {
  position: bigint | null;
  events: EmittedEventReceipt[];
};

export type CommandExecutor = {
  execute(
    commandType: string,
    input: object,
    context?: {
      correlationId?: string;
      triggeringEventId?: string;
      idempotencyKey?: string;
    },
  ): CommandReceipt;
};

// ─── DCB Query Types (internal, mirrors WIT) ─────────────────────────────────

export type EventFilter = {
  types: string[];
  tags: string[];
};

export type EventQuery = {
  items: EventFilter[];
};

// ─── Domain ID Bindings (internal) ───────────────────────────────────────────

export type DomainIdBindings = Record<string, string[]>;
