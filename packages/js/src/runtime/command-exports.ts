// `exportCommand(def)` returns `{ schema, execute }` matching the WIT
// `command` world. Wired by user code as:
//
//   export const { schema, execute } = exportCommand(MyCommand);

import type { CommandDefinition, FoldsMap, FoldStates } from "../command.js";
import type { BoundFold } from "../fold.js";
import type { EmittedEventPayload } from "../event.js";
import { InvalidInputError, RejectError, reject, invalidInput } from "../errors.js";
import { liftCommandContext, lowerEmitEvent, type CommandContext } from "../util/lift-lower.js";
import { buildDcbQuery, extractBindings } from "../util/bindings.js";
import { eventIdFromCausation } from "../util/idempotency.js";
import { setCurrentEventContext } from "./current-event-context.js";
import { initFoldStates, runFolds, type FoldStateMap } from "./fold-runner.js";
import { Transaction } from "./transaction-bindings.js";
import type {
  Result,
  WitCommandContext,
  WitCommandError,
  WitEmitEvent,
  WitExecuteOutput,
} from "../types/wit.js";

/** Returned to jco — implements the WIT `command` world. */
export interface CommandExports {
  schema(): string | undefined;
  execute(input: string, context: WitCommandContext): Result<WitExecuteOutput, WitCommandError>;
}

/** Wrap a `CommandDefinition` into WIT-shaped exports. */
export function exportCommand<TInput extends object, TFolds extends FoldsMap>(
  def: CommandDefinition<TInput, TFolds>,
): CommandExports {
  return {
    schema(): string | undefined {
      return def.input?.jsonSchema?.();
    },
    execute(rawInput, witCtx): Result<WitExecuteOutput, WitCommandError> {
      let input: TInput;
      try {
        const parsed = JSON.parse(rawInput) as unknown;
        input = def.input ? def.input.parse(parsed) : (parsed as TInput);
      } catch (err) {
        return errResult({ tag: "invalid-input", val: String((err as Error).message ?? err) });
      }
      const ctx = liftCommandContext(witCtx);
      try {
        return runCommand(def, input, ctx, witCtx);
      } catch (err) {
        if (err instanceof RejectError) return errResult({ tag: "rejected", val: err.message });
        if (err instanceof InvalidInputError)
          return errResult({ tag: "invalid-input", val: err.message });
        // Anything else traps the actor.
        throw err;
      }
    },
  };
}

function okResult(v: WitExecuteOutput): Result<WitExecuteOutput, WitCommandError> {
  return { tag: "ok", val: v };
}

function errResult(err: WitCommandError): Result<WitExecuteOutput, WitCommandError> {
  return { tag: "err", val: err };
}

function runCommand<TInput extends object, TFolds extends FoldsMap>(
  def: CommandDefinition<TInput, TFolds>,
  input: TInput,
  ctx: CommandContext,
  witCtx: WitCommandContext,
): Result<WitExecuteOutput, WitCommandError> {
  // The DCB query uses every command-level domain-id binding (Rust pattern:
  // `build_dcb_query(domain_ids, slice::from_ref(&self.input.domain_ids()))`).
  const cmdBindings = extractBindings(
    def.domainIds as readonly string[],
    input as unknown as Record<string, unknown>,
  );

  const folds = def.folds(input) as Readonly<Record<string, BoundFold<unknown>>>;
  const states = initFoldStates(folds);

  // Union of every fold's event entries. Per-fold filtering happens during
  // `runFolds` using each fold's *own* bindings.
  const entries = Object.values(folds).flatMap((f) => f.entries);
  const query = buildDcbQuery(entries, entries.length === 0 ? [] : [cmdBindings]);

  const tx = new Transaction(query);

  let isIdempotent = false;
  runFolds(tx, states, {
    idempotencyKey: ctx.idempotencyKey,
    onIdempotent: () => {
      isIdempotent = true;
    },
  });

  if (isIdempotent) {
    const position = tx.commit(witCtx, []);
    return okResult({ position, events: [] });
  }

  const restore = setCurrentEventContext({
    correlationId: ctx.correlationId,
    triggeringEventId: ctx.triggeringEventId ?? "",
  });
  let emitted: EmittedEventPayload[] | undefined;
  try {
    emitted = def.execute({
      input,
      folds: extractFoldStates<TFolds>(states),
      context: ctx,
      emit: (...events) => events,
      reject,
      invalidInput,
    });
  } finally {
    restore();
  }

  const witEvents: WitEmitEvent[] = (emitted ?? []).map((e, i) => {
    const id = eventIdFromCausation(ctx.correlationId, ctx.causationId, i);
    return lowerEmitEvent({
      id,
      eventType: e.type,
      data: e.data,
      domainIds: e.domainIds,
      encryptionScope: e.encryptionScope,
    });
  });

  const position = tx.commit(witCtx, witEvents);
  return okResult({
    position,
    events: witEvents.map((e) => ({
      id: e.id,
      eventType: e.eventType,
      domainIds: e.domainIds,
    })),
  });
}

function extractFoldStates<TFolds extends FoldsMap>(states: FoldStateMap): FoldStates<TFolds> {
  const out = {} as Record<string, unknown>;
  for (const [key, slot] of states) out[key] = slot.state;
  return out as FoldStates<TFolds>;
}
