# JavaScript SDK Implementation Plan

## Overview

This plan describes the implementation of `@umari/js` — a TypeScript-first library that
allows users to write umari modules (commands, projectors, policies, effects) in JavaScript
or TypeScript, compiled to WASM components via ComponentizeJS.

The API mirrors the Rust SDK's conceptual model exactly:
- **Events** carry domain ID field metadata
- **Folds** are pure reducers that accumulate state from events
- **Rules** are validators that carry their own fold state and contribute to the DCB query
- **Commands** declare `state` folds (for idempotency/emit logic) separately from `rules`
- **Projectors / Policies / Effects** declare event subscriptions and handle events

Everything compiles to WASM components satisfying the existing WIT interfaces unchanged.

---

## Repository Layout

Create a new top-level `packages/` directory in the umari monorepo:

```
packages/
└── js/
    ├── package.json
    ├── tsconfig.json
    ├── tsconfig.build.json
    └── src/
        ├── index.ts          — public re-exports
        ├── types.ts          — shared internal types + public TypeScript types
        ├── event.ts          — defineEvent()
        ├── fold.ts           — defineFold()
        ├── rule.ts           — defineRule()
        ├── command.ts        — defineCommand() + exportCommand()
        ├── projector.ts      — defineProjector() + exportProjector()
        ├── policy.ts         — definePolicy() + exportPolicy()
        ├── effect.ts         — defineEffect() + exportEffect()
        ├── db.ts             — SQLite wrapper over WIT sqlite imports
        ├── executor.ts       — command executor wrapper over WIT executor imports
        ├── http.ts           — fetch-compatible wrapper over WASI HTTP imports
        ├── query.ts          — DCB query building (cartesian product logic)
        └── apply.ts          — fold application + matchesFoldQuery logic
```

`package.json` name: `@umari/js`
Peer dependency: `zod` (for input schemas)
Dev dependency: `typescript`, ComponentizeJS toolchain
Target: ESM, no CJS needed (WASM modules are ESM)

---

## Part 1: Type System

This is the backbone. Get these right and everything else falls into place.

### `types.ts`

```typescript
// ─── Emitted Event (output of create()) ─────────────────────────────────────

export type EmittedEvent = {
  eventType: string;
  data: string;          // JSON-serialized
  domainIds: Array<{ name: string; id: string | null }>;
};

// ─── Stored Event (input from host) ─────────────────────────────────────────

export type StoredEventRaw = {
  id: string;
  position: bigint;
  eventType: string;
  tags: string[];
  timestamp: bigint;       // milliseconds since epoch
  correlationId: string;
  causationId: string;
  triggeringEventId: string | null;
  idempotencyKey: string | null;
  data: string;            // JSON string
};

// ─── Typed Event (what apply/handle receive) ────────────────────────────────

export type TypedEvent<TType extends string, TData extends object> = {
  readonly type: TType;
  readonly data: TData;
  readonly id: string;
  readonly position: bigint;
  readonly tags: string[];
  readonly timestamp: number;     // ms, converted from bigint
  readonly correlationId: string;
  readonly causationId: string;
  readonly triggeringEventId: string | null;
  readonly idempotencyKey: string | null;
};

// ─── Event Definition ────────────────────────────────────────────────────────

export type EventDef<
  TType extends string,
  TData extends object,
  TDomainIds extends readonly string[]
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
  TState
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
  TInput extends object
> = {
  readonly _tag: "rule";
  readonly _kind: "single";
  readonly _fold: TFold;
  readonly _check: (state: StateOf<TFold>, input: TInput) => string | null | undefined | void;
};

export type MultiFoldRule<
  TFolds extends Record<string, FoldDef<any, any>>,
  TInput extends object
> = {
  readonly _tag: "rule";
  readonly _kind: "multi";
  readonly _folds: TFolds;
  readonly _check: (states: StatesOf<TFolds>, input: TInput) => string | null | undefined | void;
};

export type RuleDef<TInput extends object = any> =
  | SingleFoldRule<FoldDef<any, any>, TInput>
  | MultiFoldRule<Record<string, FoldDef<any, any>>, TInput>;

// ─── Command Definition ──────────────────────────────────────────────────────

export type CommandDef<
  TInput extends object,
  TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>>,
  TRules extends readonly RuleDef<TInput>[]
> = {
  readonly _tag: "command";
  readonly _schema: import("zod").ZodType<TInput>;
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
  E extends EventDef<any, any, any> ? E :
  E extends { event: infer D } ? D : never;

export type EventUnionFromEntries<T extends readonly AnyEventEntry[]> = {
  [K in keyof T]: EntryEventDef<T[K]> extends EventDef<infer Type, infer Data, any>
    ? TypedEvent<Type, Data>
    : never;
}[number];

// ─── Projector Definition ────────────────────────────────────────────────────

export type ProjectorDef<TEvents extends readonly AnyEventEntry[]> = {
  readonly _tag: "projector";
  readonly _events: TEvents;
  readonly _setup?: (db: SqliteDb) => void;
  readonly _handle: (event: EventUnionFromEntries<TEvents>, db: SqliteDb) => void;
};

// ─── Policy Definition ───────────────────────────────────────────────────────

export type CommandSubmission = {
  type: string;
  input: object;
};

export type PolicyDef<TEvents extends readonly AnyEventEntry[]> = {
  readonly _tag: "policy";
  readonly _events: TEvents;
  readonly _setup?: (db: SqliteDb) => void;
  readonly _handle: (
    event: EventUnionFromEntries<TEvents>,
    db: SqliteDb
  ) => CommandSubmission[] | void;
};

// ─── Effect Definition ───────────────────────────────────────────────────────

export type EffectContext = {
  executor: CommandExecutor;
  fetch: typeof fetch;
};

export type EffectDef<TEvents extends readonly AnyEventEntry[]> = {
  readonly _tag: "effect";
  readonly _events: TEvents;
  readonly _partitionKey?: (event: EventUnionFromEntries<TEvents>) => string | null | undefined;
  readonly _handle: (event: EventUnionFromEntries<TEvents>, ctx: EffectContext) => Promise<void>;
};

// ─── SQLite DB Wrapper ───────────────────────────────────────────────────────

export type SqliteRow = Record<string, string | number | bigint | Uint8Array | null>;

export type SqliteDb = {
  execute(sql: string, params?: unknown[]): number;
  executeBatch(sql: string): void;
  queryOne(sql: string, params?: unknown[]): SqliteRow | null;
  query(sql: string, params?: unknown[]): SqliteRow[];
  lastInsertRowId(): number;
  prepare(sql: string): SqliteStatement;
};

export type SqliteStatement = {
  execute(params?: unknown[]): number;
  query(params?: unknown[]): SqliteRow[];
  queryOne(params?: unknown[]): SqliteRow | null;
};

// ─── Command Executor Wrapper ────────────────────────────────────────────────

export type CommandReceipt = {
  eventIds: string[];
};

export type CommandExecutor = {
  execute(commandType: string, input: object): CommandReceipt;
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
```

---

## Part 2: `event.ts` — `defineEvent()`

```typescript
import type { EventDef, EmittedEvent } from "./types.ts";

export function defineEvent<
  TType extends string,
  TData extends object,
  TDomainIds extends readonly string[]
>(
  type: TType,
  options: { domainIds: TDomainIds }
): EventDef<TType, TData, TDomainIds> {
  return {
    type,
    domainIds: options.domainIds,
    create(data: TData): EmittedEvent {
      return {
        eventType: type,
        data: JSON.stringify(data),
        domainIds: options.domainIds.map((field) => {
          const id = (data as Record<string, unknown>)[field];
          return {
            name: field,
            id: id != null ? String(id) : null,
          };
        }),
      };
    },
  };
}
```

Key behaviour of `create()`:
- Iterates `domainIds` fields
- Extracts each value from `data` by field name
- Converts to string (numbers, UUIDs, etc. all become strings)
- Returns `{ name, id: string | null }` pairs — matching the WIT `domain-id` record

---

## Part 3: `fold.ts` — `defineFold()`

```typescript
import type { EventDef, FoldDef, EventUnion } from "./types.ts";

export function defineFold<
  TEvents extends readonly EventDef<any, any, any>[],
  TState
>(options: {
  events: TEvents;
  initial: TState | (() => TState);
  apply: (state: TState, event: EventUnion<TEvents>) => TState;
}): FoldDef<TEvents, TState> {
  return {
    _tag: "fold",
    _events: options.events,
    _initial: options.initial,
    _apply: options.apply,
  };
}
```

The `initial` field accepts both primitive values (`false`, `0`, `""`) and factory
functions (`() => new Set()`). The apply layer (see `apply.ts`) always calls
`typeof initial === "function" ? initial() : initial` to get the starting value,
preventing shared reference bugs with objects/arrays.

---

## Part 4: `rule.ts` — `defineRule()`

Two overloads: single fold, or named object of folds.

```typescript
import type {
  FoldDef, StatesOf, StateOf,
  SingleFoldRule, MultiFoldRule, RuleDef
} from "./types.ts";

// Overload 1: single fold
export function defineRule<TFold extends FoldDef<any, any>, TInput extends object>(
  fold: TFold,
  check: (state: StateOf<TFold>, input: TInput) => string | null | undefined | void
): SingleFoldRule<TFold, TInput>;

// Overload 2: named object of folds
export function defineRule<
  TFolds extends Record<string, FoldDef<any, any>>,
  TInput extends object
>(
  folds: TFolds,
  check: (states: StatesOf<TFolds>, input: TInput) => string | null | undefined | void
): MultiFoldRule<TFolds, TInput>;

export function defineRule(foldsOrFold: any, check: any): RuleDef {
  if (foldsOrFold._tag === "fold") {
    return { _tag: "rule", _kind: "single", _fold: foldsOrFold, _check: check };
  }
  return { _tag: "rule", _kind: "multi", _folds: foldsOrFold, _check: check };
}
```

---

## Part 5: `query.ts` — DCB Query Building

This is the core algorithm. It replicates `build_query_items_from_domain_ids` from the
Rust codebase. The inputs are:
- `foldDefs`: all folds whose events need to be queried (from both `state` and `rules`)
- `bindings`: `{ shop_id: ["123"], plan_id: ["abc"] }` extracted from command input

### Algorithm

```typescript
import type {
  FoldDef, RuleDef, EventDef,
  EventQuery, EventFilter, DomainIdBindings,
  AnyEventEntry
} from "./types.ts";

/**
 * Collect all EventDefs from command state folds + rule folds.
 */
export function collectFoldEvents(
  stateFolds: Record<string, FoldDef<any, any>>,
  rules: readonly RuleDef[]
): EventDef<any, any, any>[] {
  const events: EventDef<any, any, any>[] = [];

  for (const fold of Object.values(stateFolds)) {
    events.push(...fold._events);
  }

  for (const rule of rules) {
    if (rule._kind === "single") {
      events.push(...rule._fold._events);
    } else {
      for (const fold of Object.values(rule._folds)) {
        events.push(...(fold as FoldDef<any, any>)._events);
      }
    }
  }

  return events;
}

/**
 * Build DomainIdBindings from a command input object and its declared domainIds.
 * Each domain ID maps to a single-element array (commands always have scalar inputs).
 */
export function extractBindings(
  domainIds: readonly string[],
  input: Record<string, unknown>
): DomainIdBindings {
  const bindings: DomainIdBindings = {};
  for (const field of domainIds) {
    const value = input[field];
    if (value != null) {
      bindings[field] = [String(value)];
    }
  }
  return bindings;
}

/**
 * Build the full EventQuery from a list of EventDefs and domain ID bindings.
 *
 * Steps:
 * 1. Deduplicate events by type (avoid querying same event type twice)
 * 2. Group events by their domain ID field signature (sorted join of domainIds)
 * 3. For each group, compute "effective bindings" = event's domainIds ∩ provided bindings
 * 4. Build cartesian product of effective bindings → one EventFilter per combination
 * 5. If a group's effective bindings are empty (no overlap with command input), produce
 *    a single filter with no tags (fetch all events of those types)
 * 6. Merge groups that produce identical tag sets into a single filter
 *
 * This is the JavaScript equivalent of build_query_items_from_domain_ids() in command.rs.
 */
export function buildQueryItems(
  events: EventDef<any, any, any>[],
  bindings: DomainIdBindings
): EventQuery {
  // 1. Deduplicate by event type (keep first occurrence)
  const seen = new Set<string>();
  const deduped: EventDef<any, any, any>[] = [];
  for (const event of events) {
    if (!seen.has(event.type)) {
      seen.add(event.type);
      deduped.push(event);
    }
  }

  // 2. Group events by their domain ID signature
  //    Signature = sorted domainIds joined (e.g. "order_id,shop_id")
  const groups = new Map<string, EventDef<any, any, any>[]>();
  for (const event of deduped) {
    const sig = [...event.domainIds].sort().join(",");
    if (!groups.has(sig)) groups.set(sig, []);
    groups.get(sig)!.push(event);
  }

  // 3+4+5. For each group, build effective bindings and cartesian product
  const filtersMap = new Map<string, EventFilter>();

  for (const [, group] of groups) {
    const eventDomainIds = group[0].domainIds;
    const types = group.map((e) => e.type);

    // Effective bindings: only domain ID fields that exist in the input bindings
    const effectiveFields = eventDomainIds.filter((field) => field in bindings);

    let tagCombinations: string[][];
    if (effectiveFields.length === 0) {
      // No overlap — fetch all events of these types, no tag filter
      tagCombinations = [[]];
    } else {
      // Cartesian product of each field's values
      tagCombinations = cartesian(
        effectiveFields.map((field) =>
          bindings[field].map((value) => `${field}:${value}`)
        )
      );
    }

    for (const tags of tagCombinations) {
      // 6. Merge filters with identical tags into a single filter (deduplicate)
      const tagKey = [...tags].sort().join("|");
      if (filtersMap.has(tagKey)) {
        // Merge event types into existing filter
        const existing = filtersMap.get(tagKey)!;
        for (const type of types) {
          if (!existing.types.includes(type)) {
            existing.types.push(type);
          }
        }
      } else {
        filtersMap.set(tagKey, { types: [...types], tags: [...tags] });
      }
    }
  }

  return { items: Array.from(filtersMap.values()) };
}

/**
 * Cartesian product of arrays.
 * cartesian([["a", "b"], ["x"]]) => [["a", "x"], ["b", "x"]]
 */
function cartesian(arrays: string[][]): string[][] {
  if (arrays.length === 0) return [[]];
  const [first, ...rest] = arrays;
  const restProduct = cartesian(rest);
  return first.flatMap((item) => restProduct.map((combo) => [item, ...combo]));
}

/**
 * Build EventQuery for projectors/policies/effects.
 * These don't have domain ID bindings — their query is purely based on event types
 * and optional static scope tags.
 */
export function buildQueryFromEntries(entries: readonly AnyEventEntry[]): EventQuery {
  const filtersMap = new Map<string, EventFilter>();

  for (const entry of entries) {
    const isScoped = !("type" in entry);
    const eventDef = isScoped
      ? (entry as { event: EventDef<any, any, any> }).event
      : (entry as EventDef<any, any, any>);
    const scopeTags = isScoped
      ? Object.entries((entry as { scope: Record<string, string> }).scope).map(
          ([k, v]) => `${k}:${v}`
        )
      : [];

    const tagKey = [...scopeTags].sort().join("|");

    if (filtersMap.has(tagKey)) {
      const existing = filtersMap.get(tagKey)!;
      if (!existing.types.includes(eventDef.type)) {
        existing.types.push(eventDef.type);
      }
    } else {
      filtersMap.set(tagKey, { types: [eventDef.type], tags: scopeTags });
    }
  }

  return { items: Array.from(filtersMap.values()) };
}
```

---

## Part 6: `apply.ts` — Fold Application

This replicates `FoldSet::apply()` and `matches_fold_query()` from the Rust codebase.

```typescript
import type {
  FoldDef, RuleDef, StatesOf,
  StoredEventRaw, TypedEvent, DomainIdBindings
} from "./types.ts";

/**
 * Initialise the states for a record of FoldDefs.
 */
export function initStates<T extends Record<string, FoldDef<any, any>>>(
  folds: T
): StatesOf<T> {
  const states: Record<string, unknown> = {};
  for (const [key, fold] of Object.entries(folds)) {
    states[key] =
      typeof fold._initial === "function"
        ? (fold._initial as () => unknown)()
        : fold._initial;
  }
  return states as StatesOf<T>;
}

/**
 * Initialise states for a single RuleDef (single or multi fold).
 */
export function initRuleState(rule: RuleDef): Record<string, unknown> {
  if (rule._kind === "single") {
    const fold = rule._fold;
    return {
      _state:
        typeof fold._initial === "function"
          ? (fold._initial as () => unknown)()
          : fold._initial,
    };
  } else {
    return initStates(rule._folds);
  }
}

/**
 * Deserialise a StoredEventRaw into a TypedEvent.
 */
export function deserializeEvent(raw: StoredEventRaw): TypedEvent<string, object> {
  return {
    type: raw.eventType,
    data: JSON.parse(raw.data),
    id: raw.id,
    position: raw.position,
    tags: raw.tags,
    timestamp: Number(raw.timestamp),
    correlationId: raw.correlationId,
    causationId: raw.causationId,
    triggeringEventId: raw.triggeringEventId,
    idempotencyKey: raw.idempotencyKey,
  };
}

/**
 * Check whether an event matches the fold query for a specific EventDef.
 *
 * Replicates matches_fold_query() from folds.rs:
 * - For each domain ID field declared on the event:
 *   - If the field is NOT in bindings → skip (no filtering on this field)
 *   - If the field IS in bindings → at least one of the event's tags must be "field:value"
 *     for some value in bindings[field]
 *
 * This two-tier approach is what allows the DCB query to fetch broadly
 * (only filtering by fields present in input) while fold application filters precisely.
 */
export function matchesFoldQuery(
  bindings: DomainIdBindings,
  tags: string[],
  eventDomainIds: readonly string[]
): boolean {
  for (const field of eventDomainIds) {
    const boundValues = bindings[field];
    if (!boundValues) continue; // field not in bindings → no constraint from this field

    const tagMatches = boundValues.some((value) =>
      tags.includes(`${field}:${value}`)
    );
    if (!tagMatches) return false;
  }
  return true;
}

/**
 * Apply a list of raw events to a named record of folds, using bindings for filtering.
 * Returns the accumulated states.
 */
export function applyEventsToFolds<T extends Record<string, FoldDef<any, any>>>(
  folds: T,
  events: StoredEventRaw[],
  bindings: DomainIdBindings
): StatesOf<T> {
  const states = initStates(folds);

  for (const raw of events) {
    const typed = deserializeEvent(raw);

    for (const [key, fold] of Object.entries(folds)) {
      // Find the EventDef in this fold that matches the incoming event type
      const matchingDef = (fold._events as any[]).find(
        (def: any) => def.type === raw.eventType
      );
      if (!matchingDef) continue;

      // Check domain ID bindings filter
      if (!matchesFoldQuery(bindings, raw.tags, matchingDef.domainIds)) continue;

      // Apply to this fold
      (states as Record<string, unknown>)[key] = fold._apply(
        (states as Record<string, unknown>)[key],
        typed as any
      );
    }
  }

  return states;
}

/**
 * Apply events to a single rule's fold(s) and run the check function.
 * Returns an error string, or undefined if the rule passes.
 */
export function checkRule(
  rule: RuleDef,
  events: StoredEventRaw[],
  bindings: DomainIdBindings,
  input: object
): string | undefined {
  if (rule._kind === "single") {
    const stateContainer = applyEventsToFolds(
      { _state: rule._fold },
      events,
      bindings
    );
    const result = rule._check((stateContainer as any)._state, input);
    return result ?? undefined;
  } else {
    const states = applyEventsToFolds(rule._folds, events, bindings);
    const result = rule._check(states, input);
    return result ?? undefined;
  }
}

/**
 * Run all rules in order, returning the first error found.
 */
export function checkAllRules(
  rules: readonly RuleDef[],
  events: StoredEventRaw[],
  bindings: DomainIdBindings,
  input: object
): string | undefined {
  for (const rule of rules) {
    const error = checkRule(rule, events, bindings, input);
    if (error) return error;
  }
  return undefined;
}
```

---

## Part 7: `db.ts` — SQLite Wrapper

Wraps the WIT `umari:sqlite` imports into a friendly synchronous API.

The WIT sqlite interface works synchronously (no promises). The wrapper:
1. Converts JS values to WIT `Value` variants
2. Converts WIT `Row` results to plain JS objects
3. Manages `Stmt` resources (prepares, executes, drops)

```typescript
// db.ts
// Imports from the WIT-generated bindings (generated by jco from the sqlite WIT)
import { connection, statement } from "umari:sqlite/connection@0.1.0";
import type { SqliteDb, SqliteRow, SqliteStatement } from "./types.ts";

function toWitValue(v: unknown): WitValue {
  if (v === null || v === undefined) return { tag: "null" };
  if (typeof v === "number") {
    if (Number.isInteger(v)) return { tag: "integer", val: BigInt(v) };
    return { tag: "real", val: v };
  }
  if (typeof v === "bigint") return { tag: "integer", val: v };
  if (typeof v === "string") return { tag: "text", val: v };
  if (v instanceof Uint8Array) return { tag: "blob", val: v };
  throw new Error(`unsupported sqlite param type: ${typeof v}`);
}

function fromWitRow(row: WitRow): SqliteRow {
  const obj: SqliteRow = {};
  for (const col of row.columns) {
    const v = col.value;
    switch (v.tag) {
      case "null": obj[col.name] = null; break;
      case "integer": obj[col.name] = v.val; break;
      case "real": obj[col.name] = v.val; break;
      case "text": obj[col.name] = v.val; break;
      case "blob": obj[col.name] = v.val; break;
    }
  }
  return obj;
}

export function createDb(): SqliteDb {
  return {
    execute(sql, params = []) {
      return Number(connection.execute(sql, params.map(toWitValue)));
    },
    executeBatch(sql) {
      connection.executeBatch(sql);
    },
    queryOne(sql, params = []) {
      const row = connection.queryOne(sql, params.map(toWitValue));
      return row ? fromWitRow(row) : null;
    },
    query(sql, params = []) {
      // Use prepared statement for multi-row queries
      const stmt = statement.Stmt.constructor(sql);
      try {
        const rows = stmt.query(params.map(toWitValue));
        return rows.map(fromWitRow);
      } finally {
        stmt[Symbol.dispose]?.();
      }
    },
    lastInsertRowId() {
      return Number(connection.lastInsertRowid());
    },
    prepare(sql): SqliteStatement {
      const stmt = statement.Stmt.constructor(sql);
      return {
        execute(params = []) {
          return Number(stmt.execute(params.map(toWitValue)));
        },
        query(params = []) {
          return stmt.query(params.map(toWitValue)).map(fromWitRow);
        },
        queryOne(params = []) {
          const row = stmt.queryOne(params.map(toWitValue));
          return row ? fromWitRow(row) : null;
        },
      };
    },
  };
}
```

---

## Part 8: `executor.ts` — Command Executor Wrapper

```typescript
import { executor as witExecutor } from "umari:command/executor@0.1.0";
import type { CommandExecutor, CommandReceipt } from "./types.ts";

export function createExecutor(context: {
  correlationId: string;
  triggeringEventId: string | null;
}): CommandExecutor {
  return {
    execute(commandType, input) {
      const result = witExecutor.execute(commandType, JSON.stringify(input), {
        correlationId: context.correlationId,
        triggeringEventId: context.triggeringEventId,
        idempotencyKey: null,
      });
      if (result.tag === "err") {
        throw new Error(`command execution failed: ${result.val}`);
      }
      return {
        eventIds: result.val.eventIds,
      };
    },
  };
}
```

---

## Part 9: `http.ts` — WASI HTTP Wrapper

Wraps `wasi:http/outgoing-handler` into a standard `fetch`-compatible function.
ComponentizeJS / WASI P2 already provides a polyfill for `fetch` in most cases,
so this wrapper may simply re-export the global `fetch` with a note that WASI HTTP
must be imported in the WIT world. Verify with the ComponentizeJS docs whether
`fetch` is available globally or needs an explicit wrapper.

If an explicit wrapper is needed:

```typescript
// Thin wrapper that uses WASI HTTP imports
// Full implementation depends on the jco-generated WASI HTTP bindings
export async function wasiHttp(
  url: string,
  init?: RequestInit
): Promise<Response> {
  // Use wasi:http/outgoing-handler imports to make the request
  // jco generates bindings for this; implementation follows those bindings
  // Return a standard Response object
}
```

For the initial implementation, verify whether ComponentizeJS exposes `globalThis.fetch`
backed by WASI HTTP automatically. If so, the `EffectContext.fetch` field is just
`globalThis.fetch` and this module is a no-op.

---

## Part 10: `command.ts` — `defineCommand()` + `exportCommand()`

```typescript
import { z } from "zod";
import type { CommandDef, FoldDef, RuleDef, EmittedEvent, StatesOf } from "./types.ts";
import { extractBindings, buildQueryItems, collectFoldEvents } from "./query.ts";
import { applyEventsToFolds, checkAllRules } from "./apply.ts";

export function defineCommand<
  TSchema extends z.ZodType<any>,
  TInput extends z.infer<TSchema>,
  TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>>,
  TRules extends readonly RuleDef<TInput>[]
>(def: {
  input: TSchema;
  domainIds: TDomainIds;
  state: TState;
  rules: TRules;
  emit: (states: StatesOf<TState>, input: TInput) => EmittedEvent[];
}): CommandDef<TInput, TDomainIds, TState, TRules> {
  return {
    _tag: "command",
    _schema: def.input,
    _domainIds: def.domainIds,
    _state: def.state,
    _rules: def.rules,
    _emit: def.emit,
  };
}

/**
 * Wire up the WIT exports for a command module.
 *
 * Call this once at the top level of your entry point:
 *   export const { schema, query, execute } = exportCommand(MyCommand);
 *
 * Or, if using a bundler/ComponentizeJS that expects specific named exports,
 * call it and spread into the module namespace.
 */
export function exportCommand<
  TInput extends object,
  TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>>,
  TRules extends readonly RuleDef<TInput>[]
>(def: CommandDef<TInput, TDomainIds, TState, TRules>) {
  return {
    schema(): string | null {
      // Convert Zod schema to JSON Schema using zod-to-json-schema
      // Return as JSON string
      try {
        const { zodToJsonSchema } = await import("zod-to-json-schema");
        return JSON.stringify(zodToJsonSchema(def._schema));
      } catch {
        return null;
      }
    },

    query(inputJson: string): WitEventQuery | WitError {
      let input: TInput;
      try {
        input = def._schema.parse(JSON.parse(inputJson));
      } catch (err) {
        return { tag: "err", val: { tag: "invalid-input", val: String(err) } };
      }

      const bindings = extractBindings(def._domainIds as string[], input as Record<string, unknown>);
      const allEvents = collectFoldEvents(def._state, def._rules as readonly RuleDef[]);
      const { items } = buildQueryItems(allEvents, bindings);

      return {
        tag: "ok",
        val: {
          items: items.map((item) => ({
            types: item.types,
            tags: item.tags,
          })),
        },
      };
    },

    execute(inputJson: string, rawEvents: StoredEventRaw[]): WitExecuteResult {
      let input: TInput;
      try {
        input = def._schema.parse(JSON.parse(inputJson));
      } catch (err) {
        return { tag: "err", val: { tag: "invalid-input", val: String(err) } };
      }

      const bindings = extractBindings(def._domainIds as string[], input as Record<string, unknown>);

      // Check rules (each rule applies events to its own fold(s))
      const ruleError = checkAllRules(def._rules as readonly RuleDef[], rawEvents, bindings, input);
      if (ruleError) {
        return { tag: "err", val: { tag: "rejected", val: ruleError } };
      }

      // Apply events to command state folds
      const states = applyEventsToFolds(def._state, rawEvents, bindings);

      // Call emit
      let emittedEvents: EmittedEvent[];
      try {
        emittedEvents = def._emit(states, input);
      } catch (err) {
        return { tag: "err", val: { tag: "rejected", val: String(err) } };
      }

      return {
        tag: "ok",
        val: {
          events: emittedEvents.map((e) => ({
            eventType: e.eventType,
            data: e.data,
            domainIds: e.domainIds,
          })),
        },
      };
    },
  };
}
```

**Important note on `schema()`**: The WIT `schema` export is synchronous. If
`zod-to-json-schema` needs async import, cache it at module load time instead.
Simplest approach: add `zod-to-json-schema` as a direct dependency and import it
statically at the top of the file.

---

## Part 11: `projector.ts` — `defineProjector()` + `exportProjector()`

```typescript
export function defineProjector<TEvents extends readonly AnyEventEntry[]>(def: {
  events: TEvents;
  setup?: (db: SqliteDb) => void;
  handle: (event: EventUnionFromEntries<TEvents>, db: SqliteDb) => void;
}): ProjectorDef<TEvents> { ... }

/**
 * Wire up the WIT resource exports for a projector module.
 *
 * The projector WIT interface is a resource with constructor(), query(), and handle().
 * ComponentizeJS needs a class exported as the resource implementation.
 *
 * Usage:
 *   export const Projector = exportProjector(MyProjector);
 */
export function exportProjector<TEvents extends readonly AnyEventEntry[]>(
  def: ProjectorDef<TEvents>
) {
  const db = createDb();
  let setupDone = false;

  return class {
    constructor() {}

    query(): WitEventQuery {
      const { items } = buildQueryFromEntries(def._events);
      return { items };
    }

    handle(raw: StoredEventRaw): void {
      if (!setupDone && def._setup) {
        def._setup(db);
        setupDone = true;
      }

      const typed = deserializeEvent(raw);

      // Find the matching EventDef (or scoped entry) to confirm this event is subscribed
      const isSubscribed = def._events.some((entry) => {
        const eventDef = "type" in entry ? entry : entry.event;
        return eventDef.type === raw.eventType;
      });
      if (!isSubscribed) return;

      def._handle(typed as any, db);
    }
  };
}
```

Note: `setupDone` and `db` are module-level since the WASM module is instantiated once.
The `constructor()` in the class is called by the WIT runtime when creating the resource,
but `setup` is deferred to first `handle()` call to match the Rust behaviour.

---

## Part 12: `policy.ts` — `definePolicy()` + `exportPolicy()`

Similar to projector, but `handle` returns `CommandSubmission[]`.

```typescript
export function exportPolicy<TEvents extends readonly AnyEventEntry[]>(
  def: PolicyDef<TEvents>
) {
  const db = createDb();
  let setupDone = false;

  return class {
    constructor() {}

    query(): WitEventQuery {
      return buildQueryFromEntries(def._events);
    }

    handle(raw: StoredEventRaw): WitCommandSubmission[] {
      if (!setupDone && def._setup) {
        def._setup(db);
        setupDone = true;
      }

      const typed = deserializeEvent(raw);
      const result = def._handle(typed as any, db) ?? [];

      return result.map((sub) => ({
        commandType: sub.type,
        input: JSON.stringify(sub.input),
      }));
    }
  };
}
```

---

## Part 13: `effect.ts` — `defineEffect()` + `exportEffect()`

Effects are async and have access to the command executor + HTTP.

```typescript
export function exportEffect<TEvents extends readonly AnyEventEntry[]>(
  def: EffectDef<TEvents>
) {
  return class {
    constructor() {}

    query(): WitEventQuery {
      return buildQueryFromEntries(def._events);
    }

    partitionKey(raw: StoredEventRaw): string | null {
      if (!def._partitionKey) return null;
      const typed = deserializeEvent(raw);
      return def._partitionKey(typed as any) ?? null;
    }

    async handle(raw: StoredEventRaw): Promise<void> {
      const typed = deserializeEvent(raw);

      // Build executor with event context for correlation/causation tracking
      const executor = createExecutor({
        correlationId: raw.correlationId,
        triggeringEventId: raw.id,
      });

      await def._handle(typed as any, {
        executor,
        fetch: globalThis.fetch,
      });
    }
  };
}
```

---

## Part 14: `index.ts` — Public Exports

```typescript
export { defineEvent } from "./event.ts";
export { defineFold } from "./fold.ts";
export { defineRule } from "./rule.ts";
export { defineCommand, exportCommand } from "./command.ts";
export { defineProjector, exportProjector } from "./projector.ts";
export { definePolicy, exportPolicy } from "./policy.ts";
export { defineEffect, exportEffect } from "./effect.ts";

// Public types
export type {
  EventDef,
  FoldDef,
  RuleDef,
  CommandDef,
  ProjectorDef,
  PolicyDef,
  EffectDef,
  TypedEvent,
  EmittedEvent,
  SqliteDb,
  SqliteRow,
  SqliteStatement,
  CommandExecutor,
  CommandSubmission,
  StoredEventRaw,
  EventEntry,
  AnyEventEntry,
  StateOf,
  StatesOf,
} from "./types.ts";
```

---

## Part 15: WIT Bindings Generation

ComponentizeJS requires that WIT import bindings are available at build time.
The tool `jco` (from the Bytecode Alliance) generates TypeScript bindings from WIT files.

Add a `Makefile` or `package.json` script:

```json
{
  "scripts": {
    "generate-types": "jco types ../../wit --out-dir src/generated"
  }
}
```

The generated bindings will expose the WIT imports as JS modules that can be imported
using their WIT import paths (e.g., `import { connection } from "umari:sqlite/connection@0.1.0"`).

The exact import paths and shape of the generated bindings must be verified against the
actual `jco` output for the umari WIT files before finalising `db.ts`, `executor.ts`,
and `http.ts`. The WIT files at `wit/` in the umari repo are the source of truth.

**Note**: Each module type (command, projector, policy, effect) has a different WIT world.
When compiling a user's module, the build tool must specify which world to target:
- Commands → `wit/command/world.wit`
- Projectors → `wit/projector/world.wit`
- Policies → `wit/policy/world.wit`
- Effects → `wit/effect/world.wit`

---

## Part 16: Build Tooling (`@umari/build`)

A minimal CLI that wraps ComponentizeJS:

```
packages/build/
├── package.json
└── src/
    └── cli.ts
```

Usage:
```
umari build --type command --entry src/index.ts --out dist/module.wasm
umari build --type projector --entry src/index.ts --out dist/module.wasm
```

Internally:
1. Determine the WIT world path from `--type`
2. Run ComponentizeJS with the user's entry point and the WIT world
3. Output the WASM component file

The entry point for each module type exports specific names that ComponentizeJS
picks up as the WIT exports:

- **Command**: The entry point must export `schema`, `query`, `execute` — produced
  by calling `exportCommand(def)` and spreading into the module namespace.
- **Projector / Policy / Effect**: The entry point must export the resource class
  returned by `exportProjector(def)` / `exportPolicy(def)` / `exportEffect(def)`.

The names must match what the WIT worlds expect. Verify exact export names from the
WIT files and ComponentizeJS documentation.

---

## Part 17: Complete Example Reference

The full warranti `connect-shop` command in JavaScript, for use as a test case and
as the canonical example in documentation:

```typescript
// packages/example-connect-shop/src/index.ts
import { z } from "zod";
import {
  defineEvent, defineFold, defineRule,
  defineCommand, exportCommand,
} from "@umari/js";

// ─── Events ──────────────────────────────────────────────────────────────────

const ShopConnected = defineEvent<{
  shop_id: number;
  shop_domain: string;
  shop_name: string;
  access_token: string;
}>("shop.connected", {
  domainIds: ["shop_id"] as const,
});

const ShopReconnected = defineEvent<{
  shop_id: number;
  shop_domain: string;
  shop_name: string;
  access_token: string;
}>("shop.reconnected", {
  domainIds: ["shop_id"] as const,
});

// ─── Folds ────────────────────────────────────────────────────────────────────

const shopExists = defineFold({
  events: [ShopConnected, ShopReconnected],
  initial: false,
  apply: (_state, _event) => true,
});

// ─── Command ──────────────────────────────────────────────────────────────────

const ConnectShop = defineCommand({
  input: z.object({
    shop_id: z.number(),
    shop_domain: z.string().min(1),
    shop_name: z.string().min(1),
    access_token: z.string().min(1),
  }),
  domainIds: ["shop_id"] as const,
  state: { exists: shopExists },
  rules: [],                         // ConnectShop has no rules — any shop may connect
  emit({ exists }, input) {
    if (!exists) {
      return [ShopConnected.create(input)];
    }
    return [ShopReconnected.create(input)];
  },
});

// ─── WIT Exports ──────────────────────────────────────────────────────────────

export const { schema, query, execute } = exportCommand(ConnectShop);
```

And the full `record-warranty-sale` command as a more complex example:

```typescript
import { z } from "zod";
import {
  defineEvent, defineFold, defineRule,
  defineCommand, exportCommand,
} from "@umari/js";

const WarrantyPlanCreated = defineEvent<{
  shop_id: number; plan_id: string;
  name: string; duration_months: number; price: string;
}>("warranty_plan.created", { domainIds: ["shop_id", "plan_id"] as const });

const WarrantyPlanArchived = defineEvent<{
  shop_id: number; plan_id: string;
}>("warranty_plan.archived", { domainIds: ["shop_id", "plan_id"] as const });

const WarrantySold = defineEvent<{
  shop_id: number; warranty_id: string; plan_id: string;
  order_id: number; line_item_id: number;
  customer_id: number; price: string;
  purchased_at: number; expires_at: number;
}>("warranty.sold", {
  domainIds: ["shop_id", "warranty_id", "plan_id", "order_id", "line_item_id"] as const,
});

// ShopConnected / ShopReconnected from shared events file

const shopExists = defineFold({
  events: [ShopConnected, ShopReconnected],
  initial: false,
  apply: () => true,
});

const planState = defineFold({
  events: [WarrantyPlanCreated, WarrantyPlanArchived],
  initial: "does_not_exist" as "does_not_exist" | "active" | "archived",
  apply(state, event) {
    if (event.type === "warranty_plan.created") return "active";
    if (event.type === "warranty_plan.archived") return "archived";
    return state;
  },
});

const orderSaleState = defineFold({
  events: [WarrantySold],
  initial: () => new Set<number>(),
  apply(state, event) {
    return new Set([...state, event.data.line_item_id]);
  },
});

const shopMustExist = defineRule(shopExists, (exists) => {
  if (!exists) return "shop does not exist";
});

const planMustBeActive = defineRule(planState, (state) => {
  if (state !== "active") return "plan is not active or does not exist";
});

const RecordWarrantySale = defineCommand({
  input: z.object({
    shop_id: z.number(),
    warranty_id: z.string().uuid(),
    plan_id: z.string().uuid(),
    order_id: z.number(),
    line_item_id: z.number(),
    customer_id: z.number(),
    price: z.string(),
    purchased_at: z.number(),
    expires_at: z.number(),
  }),
  domainIds: ["shop_id", "warranty_id", "plan_id", "order_id", "line_item_id"] as const,
  state: { sales: orderSaleState },
  rules: [shopMustExist, planMustBeActive],
  emit({ sales }, input) {
    if (sales.has(input.line_item_id)) {
      return []; // idempotent
    }
    return [WarrantySold.create(input)];
  },
});

export const { schema, query, execute } = exportCommand(RecordWarrantySale);
```

---

## Implementation Order

Work in this sequence. Each step is independently testable before moving on.

1. **`types.ts`** — Define all TypeScript types. No runtime logic, pure types.

2. **`event.ts`** — Implement `defineEvent()` and verify `create()` produces correct
   `domainIds` output with a unit test.

3. **`fold.ts`** — Implement `defineFold()`. No runtime logic beyond storing config.

4. **`rule.ts`** — Implement `defineRule()` overloads.

5. **`query.ts`** — Implement `buildQueryItems()` and `buildQueryFromEntries()`.
   This is the most critical algorithmic piece. Test extensively:
   - Single event, single domain ID
   - Multiple events, different domain ID signatures → separate query items
   - Cartesian product: multiple values per domain ID
   - Events with domain IDs not in the input bindings (no tag filter for those)
   - Groups that produce identical tag sets are merged
   - Projector/policy static scopes

6. **`apply.ts`** — Implement `matchesFoldQuery()`, `applyEventsToFolds()`,
   `checkAllRules()`. Test the domain ID filtering behaviour carefully:
   - Events with all required domain IDs matching → applied
   - Events where one bound domain ID doesn't match → skipped
   - Events where an unbound domain ID field is present on the event → still applied

7. **`db.ts`** — Implement after running `jco types` on the WIT files to see the
   exact shape of the generated sqlite bindings.

8. **`executor.ts`** — Same, after `jco types`.

9. **`http.ts`** — Verify whether ComponentizeJS provides `globalThis.fetch` automatically.

10. **`command.ts`** — Implement `defineCommand()` and `exportCommand()`.
    Test with the `connect-shop` example above.

11. **`projector.ts`**, **`policy.ts`**, **`effect.ts`** — Implement in any order.

12. **End-to-end test** — Compile the `connect-shop` and `record-warranty-sale`
    examples to WASM using ComponentizeJS. Load them in the umari runtime and verify
    they behave identically to their Rust equivalents.

13. **`packages/build`** — CLI wrapper once the above is working.

---

## Open Questions / Things to Verify Before Starting

1. **ComponentizeJS WIT world selection** — How exactly does ComponentizeJS know which
   WIT world to target? Is it specified via a CLI flag, a config file, or an import
   in the JS source? Check ComponentizeJS docs.

2. **`jco types` output shape** — Run `jco types ../../wit/command/world.wit` and inspect
   the generated TypeScript to understand the exact import paths and types for `db.ts`,
   `executor.ts`. The WIT import path strings used in JS imports must match exactly.

3. **`globalThis.fetch` availability** — Does ComponentizeJS / WASI P2 polyfill
   `fetch` automatically, or does `http.ts` need an explicit wrapper?

4. **Resource export format** — For projector/policy/effect, ComponentizeJS needs the
   resource class exported with the exact name expected by the WIT world. Verify whether
   it's `export class Projector`, `export { MyClass as Projector }`, or something else.

5. **`zod-to-json-schema` in WASM** — Verify this package works in a ComponentizeJS
   WASM context (no Node.js-specific APIs). Alternative: use `@zod/to-json-schema` which
   is the official Zod 4 companion.

6. **Zod version** — Use Zod 4 (`zod@^4.0.0`) if available, as it has better tree-
   shaking for WASM bundle size. If Zod 4 is not stable yet, use Zod 3 and note the
   migration path.
