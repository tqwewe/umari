import type { EventDef, StoredEventUnion } from "./event.js";

/** Options accepted by `defineEffect`. */
export interface DefineEffectOptions<
  TEvents extends readonly EventDef[],
  TState = undefined,
> {
  /** Event definitions this effect subscribes to. */
  events: TEvents;
  /** Build initial state on construction. */
  init?: () => TState;
  /**
   * Return a partition key for the event. Events sharing a key are processed
   * serially. `undefined` means the event goes through the global lane.
   */
  partitionKey?: (event: StoredEventUnion<TEvents>, state: TState) => string | undefined;
  /** Handle a single event. May be async — runtime awaits the returned promise. */
  handle: (event: StoredEventUnion<TEvents>, state: TState) => void | Promise<void>;
}

/** Pure-data definition produced by `defineEffect`. */
export interface EffectDefinition<
  TEvents extends readonly EventDef[] = readonly EventDef[],
  TState = unknown,
> {
  readonly __umariEffect: true;
  readonly events: TEvents;
  readonly init: (() => TState) | undefined;
  readonly partitionKey:
    | ((event: StoredEventUnion<TEvents>, state: TState) => string | undefined)
    | undefined;
  readonly handle: (event: StoredEventUnion<TEvents>, state: TState) => void | Promise<void>;
}

/**
 * Define an effect.
 *
 * ```ts
 * export default defineEffect({
 *   events: [ShopConnected],
 *   init: () => ({ endpoint: env('NOTIFY_ENDPOINT') }),
 *   partitionKey: (event) => event.data.shopId.toString(),
 *   handle: async (event, state) => {
 *     await fetch(state.endpoint, { method: 'POST', body: JSON.stringify({...}) });
 *   },
 * });
 * ```
 */
export function defineEffect<
  const TEvents extends readonly EventDef[],
  TState = undefined,
>(
  opts: DefineEffectOptions<TEvents, TState>,
): EffectDefinition<TEvents, TState> {
  return {
    __umariEffect: true,
    events: opts.events,
    init: opts.init,
    partitionKey: opts.partitionKey,
    handle: opts.handle,
  };
}
