import type {
  EventDef,
  FoldDef,
  FoldEventEntry,
  FoldEventUnion,
  ScopedFoldEntry,
} from "./types.ts";

/**
 * Define a fold — a pure reducer that accumulates state from a set of events.
 *
 * The `const` type parameter on TEntries means TypeScript infers the tuple type
 * from the array literal, so `as const` is not needed:
 *
 *   const shopExists = defineFold({
 *     events: [ShopConnected, ShopReconnected],
 *     initial: false,
 *     apply: (_state, _event) => true,
 *   });
 *
 * Entries can also be scoped to restrict which domain ID fields are used for
 * filtering, even if the event declares more:
 *
 *   const planNames = defineFold({
 *     events: [{ event: WarrantyPlanCreated, scope: ["shop_id"] }],
 *     initial: {} as Record<string, string>,
 *     apply: (state, event) => ({ ...state, [event.data.plan_id]: event.data.name }),
 *   });
 */
/**
 * Create a scoped fold event entry. Restricts which domain ID fields are used
 * for filtering, even if the event declares more. TypeScript infers the valid
 * field names from the event, so you get autocomplete and type-checking on scope.
 *
 *   events: [scopedEvent(WarrantyPlanCreated, ["shop_id"])]
 */
export function scopedEvent<E extends EventDef<any, any, any>>(
  event: E,
  scope: readonly E["domainIds"][number][],
): ScopedFoldEntry<E> {
  return { event, scope };
}

export function defineFold<
  const TEntries extends readonly FoldEventEntry<EventDef<any, any, any>>[],
  TState,
>(options: {
  events: TEntries;
  initial: TState | (() => TState);
  apply: (state: TState, event: FoldEventUnion<TEntries>) => TState;
}): FoldDef<TEntries, TState> {
  return {
    _tag: "fold",
    _events: options.events,
    _initial: options.initial,
    _apply: options.apply,
  };
}
