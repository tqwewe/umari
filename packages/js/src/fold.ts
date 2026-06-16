import type { EventDef, StoredEventOf, StoredEventUnion } from "./event.js";
import type { StoredEvent } from "./stored-event.js";
import type { EventDomainId } from "./util/bindings.js";

/**
 * Definition of a fold — a reducer that accumulates state from a set of
 * events.
 *
 * - `defineFold(opts)` returns a callable that, given runtime bindings,
 *   produces a `BoundFold` ready for `command.folds`, `foldQuery`, etc.
 * - The built-ins `EventFold`, `LatestEvent`, `EventCounter`, `EventToggle`
 *   are themselves fold definitions with a single `domainIds` field equal to
 *   the underlying event's `domainIds`.
 */
export interface FoldDef<
  TBindings extends object = object,
  TEvents extends readonly EventDef[] = readonly EventDef[],
  TState = unknown,
> {
  readonly __umariFold: true;
  /** Field names this fold keys on; subset of each event's domain ids. */
  readonly domainIds: readonly string[];
  /** Event definitions this fold reads. */
  readonly events: TEvents;
  /** Build the initial state. */
  initial(): TState;
  /** Apply a stored event, mutating `state` in place. */
  apply(state: TState, event: StoredEventUnion<TEvents>): void;
  /** Bind this fold to runtime values, producing a `BoundFold`. */
  (bindings: TBindings): BoundFold<TState>;
}

/** Internal handle: a fold + a binding-aware closure to `apply` events. */
export interface BoundFold<TState = unknown> {
  readonly __umariBoundFold: true;
  /** Initial state. Each replay starts from a fresh value. */
  initial(): TState;
  /** All event-type entries this bound fold reads. */
  readonly entries: readonly EventDomainId[];
  /** Runtime bindings used to filter events for this fold. */
  readonly bindings: ReadonlyMap<string, string>;
  /** Apply a stored event after the fold-runner has confirmed it matches. */
  apply(state: TState, event: StoredEvent<unknown>): void;
}

/** Internal helper used by built-ins. */
function buildEntries(
  events: readonly EventDef[],
  fieldFilter?: ReadonlySet<string>,
): EventDomainId[] {
  return events.map((ev) => ({
    eventType: ev.type,
    dynamicFields: fieldFilter
      ? ev.domainIds.filter((f) => fieldFilter.has(f))
      : ev.domainIds,
    staticFields: [] as readonly (readonly [string, string])[],
  }));
}

function encode(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number") return value.toString();
  if (typeof value === "boolean") return value ? "true" : "false";
  return String(value);
}

function bindingMap(
  bindings: object,
  fields: readonly string[],
): Map<string, string> {
  const out = new Map<string, string>();
  for (const f of fields) {
    const v = (bindings as Record<string, unknown>)[f];
    if (v === undefined || v === null) continue;
    out.set(f, encode(v));
  }
  return out;
}

// ───────────────────────── defineFold ─────────────────────────

export interface DefineFoldOptions<
  TBindings extends object,
  TEvents extends readonly EventDef[],
  TState,
> {
  /** Field names this fold keys on (subset of every event's domain ids). */
  domainIds: readonly (keyof TBindings & string)[];
  /** Event definitions to read. */
  events: TEvents;
  /** Build the initial state. */
  initial: () => TState;
  /** Apply a stored event, mutating `state` in place. */
  apply: (state: TState, event: StoredEventUnion<TEvents>) => void;
}

/**
 * Define a user fold.
 *
 * ```ts
 * const UserExistsFold = defineFold({
 *   domainIds: ['userId'] as const,
 *   events: [UserRegistered, UserReactivated],
 *   initial: () => false,
 *   apply: (state, event) => true,
 * });
 *
 * // bound:
 * UserExistsFold({ userId: 42n })
 * ```
 */
export function defineFold<
  TBindings extends object,
  const TEvents extends readonly EventDef[],
  TState,
>(
  options: DefineFoldOptions<TBindings, TEvents, TState>,
): FoldDef<TBindings, TEvents, TState> {
  const filter = new Set(options.domainIds);
  const entries = buildEntries(options.events, filter);

  const fn = ((bindings: TBindings): BoundFold<TState> => {
    const boundBindings = bindingMap(bindings, options.domainIds);
    return {
      __umariBoundFold: true,
      initial: options.initial,
      entries,
      bindings: boundBindings,
      apply(state, event) {
        // We trust the fold-runner to only deliver events that match the
        // fold's filter; just dispatch to the user's apply.
        options.apply(state as TState, event as StoredEventUnion<TEvents>);
      },
    };
  }) as FoldDef<TBindings, TEvents, TState>;

  Object.defineProperties(fn, {
    __umariFold: { value: true, enumerable: false },
    domainIds: { value: options.domainIds, enumerable: true },
    events: { value: options.events, enumerable: true },
    initial: { value: options.initial, enumerable: false },
    apply: { value: options.apply, enumerable: false },
  });

  return fn;
}

// ───────────────────────── built-in folds ─────────────────────────

/**
 * Collect all matching events into an array. Mirrors Rust `EventFold<E>` →
 * `EventState<E>`.
 *
 * ```ts
 * const projects = EventFold(ProjectCreated)({ userId, projectId });
 * ```
 */
export function EventFold<E extends EventDef>(event: E) {
  type Bindings = { [K in E["domainIds"][number]]: unknown };
  type Item = StoredEventOf<E>;
  type State = Item[];
  const entries = buildEntries([event]);
  const fn = ((bindings: Bindings): BoundFold<State> => ({
    __umariBoundFold: true,
    initial: () => [],
    entries,
    bindings: bindingMap(bindings, event.domainIds),
    apply(state, ev) {
      state.push(ev as Item);
    },
  })) as ((bindings: Bindings) => BoundFold<State>) & {
    readonly __umariFold: true;
    readonly event: E;
  };
  Object.defineProperties(fn, {
    __umariFold: { value: true, enumerable: false },
    event: { value: event, enumerable: true },
  });
  return fn;
}

/**
 * Keep only the most recent matching event. Mirrors Rust `LatestEvent<E>`.
 */
export function LatestEvent<E extends EventDef>(event: E) {
  type Bindings = { [K in E["domainIds"][number]]: unknown };
  type Item = StoredEventOf<E>;
  type State = { value: Item | undefined };
  const entries = buildEntries([event]);
  const fn = ((bindings: Bindings): BoundFold<State> => ({
    __umariBoundFold: true,
    initial: () => ({ value: undefined }),
    entries,
    bindings: bindingMap(bindings, event.domainIds),
    apply(state, ev) {
      state.value = ev as Item;
    },
  })) as ((bindings: Bindings) => BoundFold<State>) & {
    readonly __umariFold: true;
    readonly event: E;
  };
  Object.defineProperties(fn, {
    __umariFold: { value: true, enumerable: false },
    event: { value: event, enumerable: true },
  });
  return fn;
}

/** Count matching events without retaining them. Mirrors Rust `EventCounter<E>`. */
export function EventCounter<E extends EventDef>(event: E) {
  type Bindings = { [K in E["domainIds"][number]]: unknown };
  type State = { count: bigint };
  const entries = buildEntries([event]);
  const fn = ((bindings: Bindings): BoundFold<State> => ({
    __umariBoundFold: true,
    initial: () => ({ count: 0n }),
    entries,
    bindings: bindingMap(bindings, event.domainIds),
    apply(state) {
      state.count = state.count + 1n;
    },
  })) as ((bindings: Bindings) => BoundFold<State>) & {
    readonly __umariFold: true;
    readonly event: E;
  };
  Object.defineProperties(fn, {
    __umariFold: { value: true, enumerable: false },
    event: { value: event, enumerable: true },
  });
  return fn;
}

/**
 * Track which of two opposing events was most recent. Mirrors Rust
 * `EventToggle<A, B>` → `ToggleState`.
 */
export function EventToggle<A extends EventDef, B extends EventDef>(a: A, b: B) {
  // EventToggle keys on `A::DOMAIN_ID_FIELDS` (B should share them).
  type Bindings = { [K in A["domainIds"][number]]: unknown };
  type State = {
    last:
      | { side: "a"; event: StoredEventOf<A> }
      | { side: "b"; event: StoredEventOf<B> }
      | undefined;
  };
  const entries: EventDomainId[] = [
    { eventType: a.type, dynamicFields: a.domainIds, staticFields: [] },
    { eventType: b.type, dynamicFields: b.domainIds, staticFields: [] },
  ];
  const fn = ((bindings: Bindings): BoundFold<State> => ({
    __umariBoundFold: true,
    initial: () => ({ last: undefined }),
    entries,
    bindings: bindingMap(bindings, a.domainIds),
    apply(state, ev) {
      if (ev.type === a.type) {
        state.last = { side: "a", event: ev as StoredEventOf<A> };
      } else if (ev.type === b.type) {
        state.last = { side: "b", event: ev as StoredEventOf<B> };
      }
    },
  })) as ((bindings: Bindings) => BoundFold<State>) & {
    readonly __umariFold: true;
    readonly a: A;
    readonly b: B;
  };
  Object.defineProperties(fn, {
    __umariFold: { value: true, enumerable: false },
    a: { value: a, enumerable: true },
    b: { value: b, enumerable: true },
  });
  return fn;
}

/** Extract the state type a callable-fold-factory produces when bound. */
export type StateOf<T> = T extends (bindings: never) => BoundFold<infer S> ? S : never;
