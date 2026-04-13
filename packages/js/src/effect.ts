import type {
  AnyEventEntry,
  EffectContext,
  EventDef,
  EventQuery,
  EventUnionFromEntries,
  StoredEventRaw,
} from "./types.ts";
import { buildQueryFromEntries } from "./query.ts";
import { deserializeEvent } from "./apply.ts";
import { createExecutor } from "./executor.ts";
import { fetch } from "./http.ts";

/**
 * Define and export an effect module in one step.
 *
 * Returns an `Effect` class for use as a named export in your entry point:
 *
 *   export const Effect = exportEffect({
 *     events: [WarrantySold],
 *     partitionKey: (event) => event.data.shop_id,
 *     async handle(event, { fetch, executor }) {
 *       await fetch("https://...", { ... });
 *     },
 *   });
 */
export function exportEffect<
  const TEvents extends readonly AnyEventEntry[],
>(def: {
  events: TEvents;
  partitionKey?: (
    event: EventUnionFromEntries<TEvents>,
  ) => string | number | null | undefined;
  handle: (
    event: EventUnionFromEntries<TEvents>,
    ctx: EffectContext,
  ) => Promise<void>;
}) {
  return class Effect {
    constructor() {}

    query(): EventQuery {
      return buildQueryFromEntries(def.events);
    }

    partitionKey(raw: StoredEventRaw): string | undefined {
      if (!def.partitionKey) return undefined;
      const typed = deserializeEvent(raw);
      const key = def.partitionKey(typed as any);
      return key != null ? String(key) : undefined;
    }

    async handle(raw: StoredEventRaw): Promise<void> {
      const isSubscribed = def.events.some((entry) => {
        const eventDef = "type" in entry
          ? (entry as EventDef<any, any, any>)
          : (entry as { event: EventDef<any, any, any> }).event;
        return eventDef.type === raw.eventType;
      });
      if (!isSubscribed) return;

      const typed = deserializeEvent(raw);

      const executor = createExecutor({
        correlationId: raw.correlationId,
        triggeringEventId: raw.id,
      });

      const ctx: EffectContext = { executor, fetch };
      await def.handle(typed as any, ctx);
    }
  };
}
