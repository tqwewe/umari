import type { EventDef, StoredEventUnion } from "./event.js";

/** Options accepted by `defineProjector`. */
export interface DefineProjectorOptions<
  TEvents extends readonly EventDef[],
  TState = undefined,
> {
  /** Event definitions this projector subscribes to. */
  events: TEvents;
  /** Initialise persistent state (and schema, via `sqlite.executeBatch`). */
  init?: () => TState;
  /** Handle a single event. Mutate `state` in place. */
  handle: (event: StoredEventUnion<TEvents>, state: TState) => void;
}

/** Pure-data definition produced by `defineProjector`. */
export interface ProjectorDefinition<
  TEvents extends readonly EventDef[] = readonly EventDef[],
  TState = unknown,
> {
  readonly __umariProjector: true;
  readonly events: TEvents;
  readonly init: (() => TState) | undefined;
  readonly handle: (event: StoredEventUnion<TEvents>, state: TState) => void;
}

/**
 * Define a projector. State returned by `init` is mutated in place by `handle`.
 *
 * ```ts
 * export default defineProjector({
 *   events: [ShopConnected, ShopReconnected],
 *   init: () => {
 *     sqlite.executeBatch('CREATE TABLE IF NOT EXISTS shops (...);');
 *   },
 *   handle: (event) => {
 *     switch (event.type) { ... }
 *   },
 * });
 * ```
 */
export function defineProjector<
  const TEvents extends readonly EventDef[],
  TState = undefined,
>(
  opts: DefineProjectorOptions<TEvents, TState>,
): ProjectorDefinition<TEvents, TState> {
  return {
    __umariProjector: true,
    events: opts.events,
    init: opts.init,
    handle: opts.handle,
  };
}
