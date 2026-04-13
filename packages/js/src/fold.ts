import type { EventDef, EventUnion, FoldDef } from "./types.ts";

/**
 * Define a fold — a pure reducer that accumulates state from a set of events.
 *
 * The `const` type parameter on TEvents means TypeScript infers the tuple type
 * from the array literal, so `as const` is not needed:
 *
 *   const shopExists = defineFold({
 *     events: [ShopConnected, ShopReconnected],
 *     initial: false,
 *     apply: (_state, _event) => true,
 *   });
 */
export function defineFold<
  const TEvents extends readonly EventDef<any, any, any>[],
  TState,
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
