// Port of `crates/umari/src/command.rs::build_dcb_query`.
//
// A DCB (Dynamic Consistency Boundary) query groups (event-type, domain-id
// bindings) pairs by their sorted tag set. Tag = "{field}:{value}".

import type { WitEventFilter, WitEventQuery } from "../types/wit.js";

/** A field on an event marked as a domain id. */
export interface EventDomainId {
  /** The event type string. */
  eventType: string;
  /** Field names whose values are taken from runtime bindings. */
  dynamicFields: readonly string[];
  /** Field/value pairs that are always present (e.g. `topic:orders/paid`). */
  staticFields: readonly (readonly [string, string])[];
}

/** Runtime bindings: field name → string value. */
export type DomainIdBindings = ReadonlyMap<string, string>;

/**
 * Encode a single field as a tag. Mirrors Rust `format!("{}:{}", field, val)`.
 * Values stringify as: bigint → decimal, number (integer-valued) → decimal,
 * string → as-is.
 */
export function bindingValue(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error(`domain id value must be finite, got ${value}`);
    }
    return value.toString();
  }
  if (typeof value === "boolean") return value ? "true" : "false";
  if (value === null || value === undefined) {
    throw new Error("domain id value must not be null/undefined");
  }
  return String(value);
}

/**
 * Extract a `DomainIdBindings` map from a typed input object. The order of
 * keys follows `domainIds` declaration order (matches Rust IndexMap behaviour
 * via `serde` field iteration order). Missing or null fields are skipped —
 * mirrors Rust `serde(skip_serializing_if = "Option::is_none")` on optional
 * domain ids.
 */
export function extractBindings(
  domainIds: readonly string[],
  input: Record<string, unknown>,
): Map<string, string> {
  const out = new Map<string, string>();
  for (const field of domainIds) {
    const value = input[field];
    if (value === undefined || value === null) continue;
    out.set(field, bindingValue(value));
  }
  return out;
}

/**
 * Filter bindings to those declared on a specific fold/event (the Rust SDK's
 * `EventFold::from_domain_ids` does this filtering when constructing a bound
 * fold).
 */
export function pickBindings(
  bindings: DomainIdBindings,
  fields: readonly string[],
): Map<string, string> {
  const out = new Map<string, string>();
  for (const f of fields) {
    const v = bindings.get(f);
    if (v !== undefined) out.set(f, v);
  }
  return out;
}

/**
 * Build a DCB query. `entries` provides the (event-type, dynamic-fields,
 * static-fields) tuples — one per event the calling folds care about — and
 * `bindingSets` provides one or more bound runtime bindings (commands pass a
 * single set; `FoldQuery::fold_iter` passes many).
 *
 * Output: deterministic grouping by sorted set of `"field:value"` tags,
 * value = sorted set of event types that share those tags.
 */
export function buildDcbQuery(
  entries: readonly EventDomainId[],
  bindingSets: readonly DomainIdBindings[],
): WitEventQuery {
  // Tag set (sorted) → event type set (sorted).
  const grouped = new Map<string, Set<string>>();
  // Preserve a stable insertion order for the items list — mirrors Rust's
  // `IndexMap::entry(tags).or_default()` insertion semantics.
  const order: string[] = [];

  for (const bindings of bindingSets) {
    for (const entry of entries) {
      const tags: string[] = [];
      for (const field of entry.dynamicFields) {
        const v = bindings.get(field);
        if (v !== undefined) tags.push(`${field}:${v}`);
      }
      for (const [field, value] of entry.staticFields) {
        tags.push(`${field}:${value}`);
      }
      tags.sort();
      const key = tags.join("\x1f"); // unit separator — safe; field names don't contain it
      let types = grouped.get(key);
      if (!types) {
        types = new Set<string>();
        grouped.set(key, types);
        order.push(key);
      }
      types.add(entry.eventType);
    }
  }

  const items: WitEventFilter[] = order.map((key) => {
    const tags = key === "" ? [] : key.split("\x1f");
    const types = [...grouped.get(key)!].sort();
    return { types, tags };
  });

  return { items };
}

/**
 * Returns true if a stored event's tags satisfy a fold's domain-id
 * requirements. Mirrors `crates/umari/src/folds.rs::matches_fold_query`.
 */
export function matchesFoldQuery(
  entry: EventDomainId,
  storedTags: readonly string[],
  bindings: DomainIdBindings,
): boolean {
  for (const field of entry.dynamicFields) {
    const want = bindings.get(field);
    if (want === undefined) continue;
    const have = storedTags.some((t) => t === `${field}:${want}`);
    if (!have) return false;
  }
  for (const [field, value] of entry.staticFields) {
    const have = storedTags.some((t) => t === `${field}:${value}`);
    if (!have) return false;
  }
  return true;
}
