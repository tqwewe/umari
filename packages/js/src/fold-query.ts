// Stand-alone fold replay used inside effects (and any other module) to
// query the event store without emitting events. Mirrors the Rust
// `FoldQuery::new().fold(...).run()` builder.

import type { BoundFold } from "./fold.js";
import { Transaction } from "./runtime/transaction-bindings.js";
import { initFoldStates, runFolds } from "./runtime/fold-runner.js";
import { buildDcbQuery } from "./util/bindings.js";
import { uuidv4 } from "./util/uuid.js";
import type { WitCommandContext } from "./types/wit.js";

/** State type extracted from a bound fold. */
export type StateOfBound<F> = F extends BoundFold<infer S> ? S : never;

/** Result map: same keys as the input, values are each fold's terminal state. */
export type FoldQueryResult<T extends Readonly<Record<string, BoundFold<unknown>>>> = {
  [K in keyof T]: StateOfBound<T[K]>;
};

/**
 * Run zero-or-more bound folds against the event store and return their
 * final states.
 *
 * ```ts
 * const states = foldQuery({
 *   exists: UserExistsFold({ userId }),
 *   project: EventFold(ProjectCreated)({ projectId, userId }),
 * }).run();
 * ```
 */
export function foldQuery<T extends Readonly<Record<string, BoundFold<unknown>>>>(
  folds: T,
): { run(): FoldQueryResult<T> } {
  return {
    run(): FoldQueryResult<T> {
      const states = initFoldStates(folds);
      // Each bound fold contributes its own (entries × bindings) pair.
      const allEntries = Object.values(folds).flatMap((f) => f.entries);
      const bindingSets = Object.values(folds).map((f) => f.bindings as ReadonlyMap<string, string>);
      const query = buildDcbQuery(allEntries, bindingSets);
      const tx = new Transaction(query);
      runFolds(tx, states);
      // FoldQuery never emits — commit with a no-op context so the host can
      // release the transaction. The runtime ignores empty commits.
      const ctx: WitCommandContext = {
        correlationId: uuidv4(),
        causationId: uuidv4(),
      };
      tx.commit(ctx, []);
      const out = {} as Record<string, unknown>;
      for (const [key, slot] of states) out[key] = slot.state;
      return out as FoldQueryResult<T>;
    },
  };
}
