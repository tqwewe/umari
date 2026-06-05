// `exportProjector(def)` returns `{ Projector }` — a resource class wired
// into the `projector-world` WIT export.

import type { ProjectorDefinition } from "../projector.js";
import type { EventDef, StoredEventUnion } from "../event.js";
import { liftStoredEvent } from "../util/lift-lower.js";
import type { WitEventQuery, WitStoredEvent } from "../types/wit.js";

export interface ProjectorResource {
  query(): WitEventQuery;
  handle(event: WitStoredEvent): void;
}

/** Wrap a `ProjectorDefinition` into a WIT resource class. */
export function exportProjector<
  TEvents extends readonly EventDef[],
  TState,
>(def: ProjectorDefinition<TEvents, TState>): {
  /** The WIT `umari:projector/projector` interface export. */
  projector: { Projector: new () => ProjectorResource };
} {
  const subscribed = new Set(def.events.map((e) => e.type));

  class Projector implements ProjectorResource {
    #state: TState;
    constructor() {
      this.#state = (def.init ? def.init() : (undefined as unknown as TState));
    }
    query(): WitEventQuery {
      return { items: [{ types: [...subscribed], tags: [] }] };
    }
    handle(rawEvent: WitStoredEvent): void {
      if (!subscribed.has(rawEvent.eventType)) return;
      const stored = liftStoredEvent<unknown>(rawEvent) as StoredEventUnion<TEvents>;
      def.handle(stored, this.#state);
    }
  }

  return { projector: { Projector } };
}
