// Module-scope holder for "the event currently being handled". Mirrors
// `CURRENT_EVENT_CONTEXT` (a `RefCell` thread-local) in
// `crates/umari/src/command.rs:252-265`. When inside an effect handler,
// `CommandContext.new()` inherits `correlationId` from the triggering event
// and sets `triggeringEventId` to the current event id.

import { uuidv4 } from "../util/uuid.js";
import type { CommandContext } from "../util/lift-lower.js";

interface CurrentEventCtx {
  correlationId: string;
  triggeringEventId: string;
}

let CURRENT: CurrentEventCtx | undefined;

/** Set the current event context. Returns a restore function. */
export function setCurrentEventContext(ctx: CurrentEventCtx): () => void {
  const prev = CURRENT;
  CURRENT = ctx;
  return () => {
    CURRENT = prev;
  };
}

/**
 * Build a `CommandContext` for a downstream `execute(...)` call. If we are
 * inside an effect handler, inherits correlation + triggering ids.
 */
export function deriveCommandContext(): CommandContext {
  if (CURRENT) {
    return {
      correlationId: CURRENT.correlationId,
      causationId: uuidv4(),
      triggeringEventId: CURRENT.triggeringEventId,
    };
  }
  return {
    correlationId: uuidv4(),
    causationId: uuidv4(),
  };
}
