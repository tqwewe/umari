// `exportEffect(def)` returns `{ Effect }` — a resource class wired into the
// `effect-world` WIT export.
//
// Effect handlers may be `async`. The host's `handle` is a synchronous WIT
// `func() -> ()` signature, so the runtime takes care of polling the
// returned promise (StarlingMonkey blocks until the microtask queue
// drains). If your handler awaits a real `fetch` or other wasi:io operation,
// jco's componentize step generates the necessary poll glue.

import type { EffectDefinition } from "../effect.js";
import type { EventDef, StoredEventUnion } from "../event.js";
import { liftStoredEvent } from "../util/lift-lower.js";
import { setCurrentEventContext } from "./current-event-context.js";
import type { WitEventQuery, WitStoredEvent } from "../types/wit.js";

export interface EffectResource {
  query(): WitEventQuery;
  partitionKey(event: WitStoredEvent): string | undefined;
  handle(event: WitStoredEvent): void | Promise<void>;
}

/** Wrap an `EffectDefinition` into a WIT resource class. */
export function exportEffect<
  TEvents extends readonly EventDef[],
  TState,
>(def: EffectDefinition<TEvents, TState>): {
  /** The WIT `umari:effect/effect` interface export. */
  effect: { Effect: new () => EffectResource };
} {
  const subscribed = new Set(def.events.map((e) => e.type));

  class Effect implements EffectResource {
    #state: TState;
    constructor() {
      this.#state = (def.init ? def.init() : (undefined as unknown as TState));
    }
    query(): WitEventQuery {
      return { items: [{ types: [...subscribed], tags: [] }] };
    }
    partitionKey(rawEvent: WitStoredEvent): string | undefined {
      if (!subscribed.has(rawEvent.eventType)) return undefined;
      if (!def.partitionKey) return undefined;
      const stored = liftStoredEvent<unknown>(rawEvent) as StoredEventUnion<TEvents>;
      return def.partitionKey(stored, this.#state);
    }
    handle(rawEvent: WitStoredEvent): void | Promise<void> {
      if (!subscribed.has(rawEvent.eventType)) return;
      const stored = liftStoredEvent<unknown>(rawEvent) as StoredEventUnion<TEvents>;
      const restore = setCurrentEventContext({
        correlationId: stored.correlationId,
        triggeringEventId: stored.id,
      });
      try {
        const result = def.handle(stored, this.#state);
        if (result && typeof (result as Promise<void>).then === "function") {
          return (result as Promise<void>).finally(restore);
        }
        restore();
        return undefined;
      } catch (err) {
        restore();
        throw err;
      }
    }
  }

  return { effect: { Effect } };
}
