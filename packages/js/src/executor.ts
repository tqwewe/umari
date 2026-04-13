// executor.ts — Command executor wrapper over WIT executor imports
// Import path is resolved by ComponentizeJS at build time from the WIT world.
// TypeScript resolves this via tsconfig paths → src/generated/command/interfaces/
import * as witExecutor from "umari:command/executor@0.1.0";
import type { CommandExecutor, CommandReceipt } from "./types.ts";

export function createExecutor(context: {
  correlationId?: string;
  triggeringEventId?: string;
  idempotencyKey?: string;
}): CommandExecutor {
  return {
    execute(commandType, input, overrides = {}) {
      const correlationId = overrides.correlationId ?? context.correlationId;
      const triggeringEventId = overrides.triggeringEventId ?? context.triggeringEventId;
      const idempotencyKey = overrides.idempotencyKey ?? context.idempotencyKey;

      const receipt = witExecutor.execute(
        commandType,
        JSON.stringify(input),
        {
          ...(correlationId !== undefined && { correlationId }),
          ...(triggeringEventId !== undefined && { triggeringEventId }),
          ...(idempotencyKey !== undefined && { idempotencyKey }),
        },
      );
      return {
        position: receipt.position ?? null,
        events: receipt.events.map((e) => ({
          id: e.id,
          eventType: e.eventType,
          tags: e.tags,
        })),
      } satisfies CommandReceipt;
    },
  };
}
