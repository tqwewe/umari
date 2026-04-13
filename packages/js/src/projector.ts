import type {
  AnyEventEntry,
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
 * Define and export a projector module in one step.
 *
 * Returns a `Projector` class for use as a named export in your entry point:
 *
 *   export const Projector = exportProjector({
 *     events: [ShopConnected, ShopReconnected],
 *     setup(db) { db.executeBatch("CREATE TABLE IF NOT EXISTS ..."); },
 *     handle(event, db) { ... },
 *   });
 */
export function exportProjector<
  const TEvents extends readonly AnyEventEntry[],
>(def: {
  events: TEvents;
  setup?: (db: SqliteDb) => void;
  handle: (event: EventUnionFromEntries<TEvents>, db: SqliteDb) => void;
}) {
  // db and setupDone are module-level — WASM module is instantiated once
  const db = createDb();
  let setupDone = false;

  return class Projector {
    constructor() {}

    query(): EventQuery {
      return buildQueryFromEntries(def.events);
    }

    handle(raw: StoredEventRaw): void {
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
      if (!isSubscribed) return;

      const typed = deserializeEvent(raw);
      def.handle(typed as any, db);
    }
  };
}
