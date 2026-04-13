import { zodToJsonSchema } from "zod-to-json-schema";

import type {
  CommandDef,
  EmittedEvent,
  EventFilter,
  FoldDef,
  RuleDef,
  StatesOf,
  StoredEventRaw,
} from "./types.ts";
import {
  buildQueryItems,
  collectFoldEvents,
  extractBindings,
} from "./query.ts";
import { applyEventsToFolds, checkAllRules } from "./apply.ts";

/**
 * Define a command. `state` and `rules` are optional — omit them when not needed:
 *
 *   const ConnectShop = defineCommand({
 *     input: z.object({ shop_id: z.number() }),
 *     domainIds: ["shop_id"],
 *     emit(_, input) { return [ShopConnected.create(input)]; },
 *   });
 */
export function defineCommand<
  TInput extends object,
  const TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>> = Record<never, never>,
  TRules extends readonly RuleDef<TInput>[] = readonly [],
>(def: {
  input: { parse(data: unknown): TInput };
  domainIds: TDomainIds;
  state?: TState;
  rules?: TRules;
  emit: (states: StatesOf<TState>, input: TInput) => EmittedEvent[];
}): CommandDef<TInput, TDomainIds, TState, TRules> {
  return {
    _tag: "command",
    _schema: def.input,
    _domainIds: def.domainIds,
    _state: (def.state ?? {}) as TState,
    _rules: (def.rules ?? []) as unknown as TRules,
    _emit: def.emit,
  };
}

// ─── WIT export shape ────────────────────────────────────────────────────────
// These types mirror the jco-generated command world exports.
// query() and execute() throw on error (jco maps WIT result<T,E> to throw).

type WitEventQuery = { items: Array<{ types: string[]; tags: string[] }> };

type WitEmittedEvent = {
  eventType: string;
  data: string;
  domainIds: Array<{ name: string; id?: string }>;
};

type WitExecuteOutput = { events: WitEmittedEvent[] };

/**
 * Wire up the WIT exports for a command module.
 *
 * Usage in entry point:
 *   export const { schema, query, execute } = exportCommand(ConnectShop);
 */
export function exportCommand<
  TInput extends object,
  TDomainIds extends readonly (keyof TInput & string)[],
  TState extends Record<string, FoldDef<any, any>>,
  TRules extends readonly RuleDef<TInput>[],
>(def: CommandDef<TInput, TDomainIds, TState, TRules>) {
  return {
    // Returns undefined if no schema (matches jco: option<json> → Json | undefined)
    schema(): string | undefined {
      try {
        return JSON.stringify(zodToJsonSchema(def._schema as any));
      } catch {
        return undefined;
      }
    },

    // Throws on invalid input (jco maps result<T,E> to throw)
    query(inputJson: string): WitEventQuery {
      const input = def._schema.parse(JSON.parse(inputJson)) as TInput;

      const bindings = extractBindings(
        def._domainIds as unknown as string[],
        input as Record<string, unknown>,
      );
      const allEvents = collectFoldEvents(
        def._state,
        def._rules as readonly RuleDef[],
      );
      const { items } = buildQueryItems(allEvents, bindings);

      return {
        items: items.map((item: EventFilter) => ({
          types: item.types,
          tags: item.tags,
        })),
      };
    },

    // Throws on invalid input or rule rejection
    execute(inputJson: string, rawEvents: StoredEventRaw[]): WitExecuteOutput {
      const input = def._schema.parse(JSON.parse(inputJson)) as TInput;

      const bindings = extractBindings(
        def._domainIds as unknown as string[],
        input as Record<string, unknown>,
      );

      // Check rules — each rule applies events to its own fold(s)
      const ruleError = checkAllRules(
        def._rules as readonly RuleDef[],
        rawEvents,
        bindings,
        input,
      );
      if (ruleError) {
        throw Object.assign(new Error(ruleError), { tag: "rejected", val: ruleError });
      }

      // Apply events to command state folds
      const states = applyEventsToFolds(def._state, rawEvents, bindings);

      const emittedEvents = def._emit(states, input);

      return {
        events: emittedEvents.map((e) => ({
          eventType: e.eventType,
          data: e.data,
          domainIds: e.domainIds.map((d) => ({
            name: d.name,
            ...(d.id !== null && { id: d.id }),
          })),
        })),
      };
    },
  };
}
