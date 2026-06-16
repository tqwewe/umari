import { execute as witExecute } from "umari:command/executor@0.1.0";
import { stringifyPayload } from "../util/bigint-json.js";
import {
  lowerCommandContext,
  type CommandContext,
} from "../util/lift-lower.js";
import { deriveCommandContext } from "./current-event-context.js";

/**
 * Invoke another command by name. Available only in module worlds that
 * import `umari:command/executor` (commands and effects).
 *
 * ```ts
 * execute('create-project', { userId, ... }, {
 *   correlationId: event.correlationId,
 *   triggeringEventId: event.id,
 *   idempotencyKey: event.id,
 * });
 * ```
 *
 * Context defaults: if omitted, derives from the current event (effects) or
 * generates fresh ids.
 */
export function execute(
  command: string,
  input: unknown,
  context?: Partial<CommandContext>,
): void {
  const derived = deriveCommandContext();
  const ctx: CommandContext = {
    correlationId: context?.correlationId ?? derived.correlationId,
    causationId: context?.causationId ?? derived.causationId,
    triggeringEventId: context?.triggeringEventId ?? derived.triggeringEventId,
    idempotencyKey: context?.idempotencyKey,
  };
  witExecute(command, stringifyPayload(input), lowerCommandContext(ctx));
}
