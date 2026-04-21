import type {
  AnyEventEntry,
  DomainIdBindings,
  EventDef,
  EventFilter,
  EventQuery,
  FoldDef,
  FoldEventEntry,
  RuleDef,
} from "./types.ts";

type ResolvedEventDef = { type: string; domainIds: readonly string[] };

function effectiveDomainIds(
  entry: FoldEventEntry<EventDef<any, any, any>>,
): readonly string[] {
  return "event" in entry ? entry.scope : entry.domainIds;
}

function entryEventType(
  entry: FoldEventEntry<EventDef<any, any, any>>,
): string {
  return "event" in entry ? entry.event.type : entry.type;
}

/**
 * Collect resolved event definitions (type + effective domainIds) from command
 * state folds + rule folds. Scoped entries use the scope instead of the event's
 * full domainIds.
 */
export function collectFoldEvents(
  stateFolds: Record<string, FoldDef<any, any>>,
  rules: readonly RuleDef[],
): ResolvedEventDef[] {
  const events: ResolvedEventDef[] = [];

  for (const fold of Object.values(stateFolds)) {
    for (const entry of fold._events as FoldEventEntry<
      EventDef<any, any, any>
    >[]) {
      events.push({
        type: entryEventType(entry),
        domainIds: effectiveDomainIds(entry),
      });
    }
  }

  for (const rule of rules) {
    if (rule._kind === "single") {
      for (const entry of rule._fold._events as FoldEventEntry<
        EventDef<any, any, any>
      >[]) {
        events.push({
          type: entryEventType(entry),
          domainIds: effectiveDomainIds(entry),
        });
      }
    } else {
      for (const fold of Object.values(rule._folds)) {
        for (const entry of (fold as FoldDef<any, any>)
          ._events as FoldEventEntry<EventDef<any, any, any>>[]) {
          events.push({
            type: entryEventType(entry),
            domainIds: effectiveDomainIds(entry),
          });
        }
      }
    }
  }

  return events;
}

/**
 * Build DomainIdBindings from a command input object and its declared domainIds.
 * Each domain ID maps to a single-element array (commands always have scalar inputs).
 */
export function extractBindings(
  domainIds: readonly string[],
  input: Record<string, unknown>,
): DomainIdBindings {
  const bindings: DomainIdBindings = {};
  for (const field of domainIds) {
    const value = input[field];
    if (value != null) {
      bindings[field] = [String(value)];
    }
  }
  return bindings;
}

/**
 * Build the full EventQuery from a list of resolved event definitions and domain
 * ID bindings.
 *
 * Steps:
 * 1. Deduplicate events by type (avoid querying same event type twice)
 * 2. Group events by their domain ID field signature (sorted join of domainIds)
 * 3. For each group, compute "effective bindings" = event's domainIds ∩ provided bindings
 * 4. Build cartesian product of effective bindings → one EventFilter per combination
 * 5. If a group's effective bindings are empty (no overlap with command input), produce
 *    a single filter with no tags (fetch all events of those types)
 * 6. Merge groups that produce identical tag sets into a single filter
 *
 * This is the JavaScript equivalent of build_query_items_from_domain_ids() in command.rs.
 */
export function buildQueryItems(
  events: ResolvedEventDef[],
  bindings: DomainIdBindings,
): EventQuery {
  // 1. Deduplicate by event type (keep first occurrence)
  const seen = new Set<string>();
  const deduped: ResolvedEventDef[] = [];
  for (const event of events) {
    if (!seen.has(event.type)) {
      seen.add(event.type);
      deduped.push(event);
    }
  }

  // 2. Group events by their domain ID signature
  //    Signature = sorted domainIds joined (e.g. "order_id,shop_id")
  const groups = new Map<string, ResolvedEventDef[]>();
  for (const event of deduped) {
    const sig = [...event.domainIds].sort().join(",");
    if (!groups.has(sig)) groups.set(sig, []);
    groups.get(sig)!.push(event);
  }

  // 3+4+5. For each group, build effective bindings and cartesian product
  const filtersMap = new Map<string, EventFilter>();

  for (const [, group] of groups) {
    const eventDomainIds = group[0]!.domainIds;
    const types = group.map((e) => e.type);

    // Effective bindings: only domain ID fields that exist in the input bindings
    const effectiveFields = eventDomainIds.filter((field) => field in bindings);

    let tagCombinations: string[][];
    if (effectiveFields.length === 0) {
      // No overlap — fetch all events of these types, no tag filter
      tagCombinations = [[]];
    } else {
      // Cartesian product of each field's values
      tagCombinations = cartesian(
        effectiveFields.map((field) =>
          (bindings[field] ?? []).map((value) => `${field}:${value}`),
        ),
      );
    }

    for (const tags of tagCombinations) {
      // 6. Merge filters with identical tags into a single filter (deduplicate)
      const tagKey = [...tags].sort().join("|");
      if (filtersMap.has(tagKey)) {
        // Merge event types into existing filter
        const existing = filtersMap.get(tagKey)!;
        for (const type of types) {
          if (!existing.types.includes(type)) {
            existing.types.push(type);
          }
        }
      } else {
        filtersMap.set(tagKey, { types: [...types], tags: [...tags] });
      }
    }
  }

  return { items: Array.from(filtersMap.values()) };
}

/**
 * Cartesian product of arrays.
 * cartesian([["a", "b"], ["x"]]) => [["a", "x"], ["b", "x"]]
 */
function cartesian(arrays: string[][]): string[][] {
  if (arrays.length === 0) return [[]];
  const [first, ...rest] = arrays;
  if (!first) return [[]];
  const restProduct = cartesian(rest);
  return first.flatMap((item) => restProduct.map((combo) => [item, ...combo]));
}

/**
 * Build EventQuery for projectors/effects.
 * These don't have domain ID bindings — their query is purely based on event types
 * and optional static scope tags.
 */
export function buildQueryFromEntries(
  entries: readonly AnyEventEntry[],
): EventQuery {
  const filtersMap = new Map<string, EventFilter>();

  for (const entry of entries) {
    const isScoped = !("type" in entry);
    const eventDef = isScoped
      ? (entry as { event: EventDef<any, any, any> }).event
      : (entry as EventDef<any, any, any>);
    const scopeTags = isScoped
      ? Object.entries((entry as { scope: Record<string, string> }).scope).map(
          ([k, v]) => `${k}:${v}`,
        )
      : [];

    const tagKey = [...scopeTags].sort().join("|");

    if (filtersMap.has(tagKey)) {
      const existing = filtersMap.get(tagKey)!;
      if (!existing.types.includes(eventDef.type)) {
        existing.types.push(eventDef.type);
      }
    } else {
      filtersMap.set(tagKey, { types: [eventDef.type], tags: scopeTags });
    }
  }

  return { items: Array.from(filtersMap.values()) };
}
