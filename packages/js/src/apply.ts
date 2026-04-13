import type {
  DomainIdBindings,
  EventDef,
  FoldDef,
  RuleDef,
  StatesOf,
  StoredEventRaw,
  TypedEvent,
} from "./types.ts";

/**
 * Initialise the states for a record of FoldDefs.
 */
export function initStates<T extends Record<string, FoldDef<any, any>>>(
  folds: T,
): StatesOf<T> {
  const states: Record<string, unknown> = {};
  for (const [key, fold] of Object.entries(folds)) {
    states[key] =
      typeof fold._initial === "function"
        ? (fold._initial as () => unknown)()
        : fold._initial;
  }
  return states as StatesOf<T>;
}

/**
 * Deserialise a StoredEventRaw into a TypedEvent.
 */
export function deserializeEvent(
  raw: StoredEventRaw,
): TypedEvent<string, object> {
  return {
    type: raw.eventType,
    data: JSON.parse(raw.data) as object,
    id: raw.id,
    position: raw.position,
    tags: raw.tags,
    timestamp: Number(raw.timestamp),
    correlationId: raw.correlationId,
    causationId: raw.causationId,
    triggeringEventId: raw.triggeringEventId ?? undefined,
    idempotencyKey: raw.idempotencyKey ?? undefined,
  };
}

/**
 * Check whether an event matches the fold query for a specific EventDef.
 *
 * Replicates matches_fold_query() from folds.rs:
 * - For each domain ID field declared on the event:
 *   - If the field is NOT in bindings → skip (no filtering on this field)
 *   - If the field IS in bindings → at least one of the event's tags must be "field:value"
 *     for some value in bindings[field]
 */
export function matchesFoldQuery(
  bindings: DomainIdBindings,
  tags: string[],
  eventDomainIds: readonly string[],
): boolean {
  for (const field of eventDomainIds) {
    const boundValues = bindings[field];
    if (!boundValues) continue; // field not in bindings → no constraint from this field

    const tagMatches = boundValues.some((value) =>
      tags.includes(`${field}:${value}`),
    );
    if (!tagMatches) return false;
  }
  return true;
}

/**
 * Apply a list of raw events to a named record of folds, using bindings for filtering.
 * Returns the accumulated states.
 */
export function applyEventsToFolds<T extends Record<string, FoldDef<any, any>>>(
  folds: T,
  events: StoredEventRaw[],
  bindings: DomainIdBindings,
): StatesOf<T> {
  const states = initStates(folds);

  for (const raw of events) {
    const typed = deserializeEvent(raw);

    for (const [key, fold] of Object.entries(folds)) {
      // Find the EventDef in this fold that matches the incoming event type
      const matchingDef = (fold._events as EventDef<any, any, any>[]).find(
        (def) => def.type === raw.eventType,
      );
      if (!matchingDef) continue;

      // Check domain ID bindings filter
      if (
        !matchesFoldQuery(bindings, raw.tags, matchingDef.domainIds as string[])
      )
        continue;

      // Apply to this fold
      (states as Record<string, unknown>)[key] = fold._apply(
        (states as Record<string, unknown>)[key],
        typed as any,
      );
    }
  }

  return states;
}

/**
 * Apply events to a single rule's fold(s) and run the check function.
 * Returns an error string, or undefined if the rule passes.
 */
export function checkRule(
  rule: RuleDef,
  events: StoredEventRaw[],
  bindings: DomainIdBindings,
  input: object,
): string | undefined {
  if (rule._kind === "single") {
    const stateContainer = applyEventsToFolds(
      { _state: rule._fold },
      events,
      bindings,
    );
    const result = rule._check((stateContainer as any)._state, input);
    return result ?? undefined;
  } else {
    const states = applyEventsToFolds(rule._folds, events, bindings);
    const result = rule._check(states, input);
    return result ?? undefined;
  }
}

/**
 * Run all rules in order, returning the first error found.
 */
export function checkAllRules(
  rules: readonly RuleDef[],
  events: StoredEventRaw[],
  bindings: DomainIdBindings,
  input: object,
): string | undefined {
  for (const rule of rules) {
    const err = checkRule(rule, events, bindings, input);
    if (err) return err;
  }
  return undefined;
}
