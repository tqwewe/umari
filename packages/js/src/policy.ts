import type {
  AnyEventEntry,
  CommandSubmission,
  EventDef,
  EventQuery,
  EventUnionFromEntries,
  SqliteDb,
  StoredEventRaw,
} from "./types.ts";
import { buildQueryFromEntries } from "./query.ts";
import { deserializeEvent } from "./apply.ts";
import { createDb } from "./db.ts";

/**
 * Define and export a policy module in one step.
 *
 * Returns a `Policy` class for use as a named export in your entry point:
 *
 *   export const Policy = exportPolicy({
 *     events: [WarrantySold],
 *     handle(event) {
 *       return [{ type: "send-warranty-email", input: { orderId: event.data.order_id } }];
 *     },
 *   });
 */
export function exportPolicy<
  const TEvents extends readonly AnyEventEntry[],
>(def: {
  events: TEvents;
  setup?: (db: SqliteDb) => void;
  handle: (
    event: EventUnionFromEntries<TEvents>,
    db: SqliteDb,
  ) => CommandSubmission[] | void;
}) {
  const db = createDb();
  let setupDone = false;

  return class Policy {
    constructor() {}

    query(): EventQuery {
      return buildQueryFromEntries(def.events);
    }

    handle(raw: StoredEventRaw): Array<{ commandType: string; input: string }> {
      if (!setupDone && def.setup) {
        def.setup(db);
        setupDone = true;
      }

      const isSubscribed = def.events.some((entry) => {
        const eventDef = "type" in entry
          ? (entry as EventDef<any, any, any>)
          : (entry as { event: EventDef<any, any, any> }).event;
        return eventDef.type === raw.eventType;
      });
      if (!isSubscribed) return [];

      const typed = deserializeEvent(raw);
      const result = def.handle(typed as any, db) ?? [];

      return result.map((sub: CommandSubmission) => ({
        commandType: sub.type,
        input: JSON.stringify(sub.input),
      }));
    }
  };
}
