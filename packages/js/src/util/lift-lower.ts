// Conversions between WIT host types and the high-level JS API.

import { uuidv4 } from "./uuid.js";
import type {
  WitCommandContext,
  WitDomainId,
  WitEmitEvent,
  WitEmittedEvent,
  WitExecuteOutput,
  WitStoredEvent,
} from "../types/wit.js";
import type { StoredEvent } from "../stored-event.js";
import { parsePayload, stringifyPayload } from "./bigint-json.js";

/** A `CommandContext` carried inside the JS guest. */
export interface CommandContext {
  correlationId: string;
  causationId: string;
  triggeringEventId?: string;
  idempotencyKey?: string;
}

/** Build a fresh `CommandContext` — fresh correlation+causation ids. */
export function newCommandContext(): CommandContext {
  return {
    correlationId: uuidv4(),
    causationId: uuidv4(),
  };
}

export function liftCommandContext(ctx: WitCommandContext): CommandContext {
  return {
    correlationId: ctx.correlationId,
    causationId: ctx.causationId,
    triggeringEventId: ctx.triggeringEventId,
    idempotencyKey: ctx.idempotencyKey,
  };
}

export function lowerCommandContext(ctx: CommandContext): WitCommandContext {
  return {
    correlationId: ctx.correlationId,
    causationId: ctx.causationId,
    triggeringEventId: ctx.triggeringEventId,
    idempotencyKey: ctx.idempotencyKey,
  };
}

/** Lift a WIT `stored-event` into the user-facing typed `StoredEvent`. */
export function liftStoredEvent<TData>(ev: WitStoredEvent): StoredEvent<TData> {
  const data: TData =
    ev.encryptionScope !== undefined && ev.data === "null"
      ? (null as unknown as TData)
      : parsePayload<TData>(ev.data);
  return {
    id: ev.id,
    position: ev.position,
    type: ev.eventType,
    tags: ev.tags,
    timestamp: new Date(Number(ev.timestamp)),
    correlationId: ev.correlationId,
    causationId: ev.causationId,
    triggeringEventId: ev.triggeringEventId,
    idempotencyKey: ev.idempotencyKey,
    encryptionScope: ev.encryptionScope,
    encryptionKeyId: ev.encryptionKeyId,
    data,
  };
}

/** Lower an emitted event from the public shape into the WIT record. */
export function lowerEmitEvent(args: {
  id: string;
  eventType: string;
  data: unknown;
  domainIds: ReadonlyMap<string, string>;
  encryptionScope?: string;
}): WitEmitEvent {
  return {
    id: args.id,
    eventType: args.eventType,
    data: stringifyPayload(args.data),
    domainIds: [...args.domainIds.entries()].map(([name, id]) => ({ name, id })),
    encryptionScope: args.encryptionScope,
  };
}

/** Build the `ExecuteOutput` view returned by `command.execute`. */
export function buildExecuteOutput(
  position: bigint | undefined,
  emitted: ReadonlyArray<{
    id: string;
    eventType: string;
    domainIds: ReadonlyMap<string, string>;
  }>,
): WitExecuteOutput {
  return {
    position,
    events: emitted.map((e): WitEmittedEvent => ({
      id: e.id,
      eventType: e.eventType,
      domainIds: [...e.domainIds.entries()].map(([name, id]): WitDomainId => ({ name, id })),
    })),
  };
}
