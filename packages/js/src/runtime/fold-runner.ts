// Shared event replay loop used by `command-exports` and `fold-query`. Reads
// batches from a `Transaction`, applies matching events to each bound fold,
// and short-circuits if an event has the same idempotency key as the caller
// (mirrors `crates/umari/src/command.rs:126-136`).

import type { BoundFold } from "../fold.js";
import { liftStoredEvent } from "../util/lift-lower.js";
import { buildDcbQuery, matchesFoldQuery } from "../util/bindings.js";
import { Transaction } from "./transaction-bindings.js";
import type { WitCommandContext } from "../types/wit.js";

/** State holder keyed by the same identifiers used in the user's folds map. */
export type FoldStateMap = Map<string, { fold: BoundFold<unknown>; state: unknown }>;

/** Initialise a state map for a record of bound folds. */
export function initFoldStates(
  folds: Readonly<Record<string, BoundFold<unknown>>>,
): FoldStateMap {
  const states: FoldStateMap = new Map();
  for (const [key, fold] of Object.entries(folds)) {
    states.set(key, { fold, state: fold.initial() });
  }
  return states;
}

/** Build the DCB query for a record of bound folds. */
export function buildQueryForFolds(folds: Readonly<Record<string, BoundFold<unknown>>>) {
  const entries = [];
  const bindings: ReadonlyMap<string, string>[] = [];
  for (const fold of Object.values(folds)) {
    entries.push(...fold.entries);
    bindings.push(fold.bindings);
  }
  // Use *one combined* binding set (matches the Rust command path that calls
  // `build_dcb_query(domain_ids, slice::from_ref(&bindings))`).
  const merged = new Map<string, string>();
  for (const b of bindings) for (const [k, v] of b) merged.set(k, v);
  return buildDcbQuery(entries, [merged]);
}

/** Build the DCB query when each fold has its *own* binding set (FoldQuery). */
export function buildQueryForFoldsPerSet(folds: Readonly<Record<string, BoundFold<unknown>>>) {
  // Each fold contributes its own (binding-set, entry-list) pair. We expand
  // by computing per-fold (entries × bindings) pairs and unioning.
  const allEntries = [];
  const bindingSets: ReadonlyMap<string, string>[] = [];
  for (const fold of Object.values(folds)) {
    allEntries.push(...fold.entries);
    bindingSets.push(fold.bindings);
  }
  return buildDcbQuery(allEntries, bindingSets);
}

/** Run the transaction loop, applying matching events to each bound fold. */
export function runFolds(
  tx: Transaction,
  states: FoldStateMap,
  options?: {
    /** Short-circuit when an event's `idempotencyKey` matches this. */
    idempotencyKey?: string;
    /** Called when an idempotency-key match is seen — caller decides to commit empty. */
    onIdempotent?: () => void;
  },
): "drained" | "idempotent" {
  for (;;) {
    const batch = tx.nextBatch();
    if (batch.length === 0) break;
    for (const raw of batch) {
      if (
        options?.idempotencyKey !== undefined &&
        raw.idempotencyKey !== undefined &&
        raw.idempotencyKey === options.idempotencyKey
      ) {
        options.onIdempotent?.();
        return "idempotent";
      }
      // Skip crypto-shredded events (data == "null" + scope set) — same as
      // Rust `BoxFold::box_apply` short-circuit.
      const skipShredded = raw.encryptionScope !== undefined && raw.data === "null";
      if (skipShredded) continue;

      const stored = liftStoredEvent<unknown>(raw);
      for (const slot of states.values()) {
        const entry = slot.fold.entries.find((e) => e.eventType === stored.type);
        if (!entry) continue;
        if (!matchesFoldQuery(entry, stored.tags, slot.fold.bindings)) continue;
        // Reduce-style: honour a returned next state; `undefined` means the
        // user mutated `state` in place, so keep it.
        const next = slot.fold.apply(slot.state, stored);
        if (next !== undefined) slot.state = next;
      }
    }
  }
  return "drained";
}

/** Run a transaction with the given query, then commit with the given context. */
export function runWithTransaction(
  query: ReturnType<typeof buildQueryForFolds>,
  ctx: WitCommandContext,
  body: (tx: Transaction) => void,
  commitEvents: () => Parameters<Transaction["commit"]>[1],
): bigint | undefined {
  const tx = new Transaction(query);
  body(tx);
  return tx.commit(ctx, commitEvents());
}
