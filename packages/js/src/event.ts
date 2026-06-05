import { stringifyPayload } from "./util/bigint-json.js";
import type { StoredEvent } from "./stored-event.js";

/** Phantom property carrying the event-type literal at the type level. */
export interface EventTypeBrand<TType extends string> {
  readonly __umariEventType: TType;
}

/**
 * Payload object ready to hand to `emit(...)`. The runtime layer encodes the
 * domain ids and payload JSON during `tx.commit`.
 */
export interface EmittedEventPayload<TType extends string = string> {
  readonly __umariEventPayload: true;
  /** Event type string, e.g. `"shop.connected"`. */
  readonly type: TType;
  /** Raw payload — encoded with bigint-as-string by the runtime. */
  readonly data: unknown;
  /** Field name → string id, built from the payload at emit time. */
  readonly domainIds: ReadonlyMap<string, string>;
  /** Optional encryption scope (`"shop:42"` etc.). */
  readonly encryptionScope?: string;
}

/** Options accepted by `defineEvent`. */
export interface DefineEventOptions<
  TData extends object,
  TDomainIds extends readonly (keyof TData & string)[],
> {
  /** Domain-id fields. Each must be a key of `TData` and a primitive value. */
  domainIds: TDomainIds;
  /** Encryption scope, in `"prefix:value"` form. Mirrors Rust `Event::encryption_scope`. */
  cryptoScope?: (data: TData) => string | undefined;
}

/**
 * Definition produced by `defineEvent<TData>()(type, opts)`. Callable as a
 * factory that turns `TData` into an `EmittedEventPayload`, ready for `emit`.
 *
 * The phantom `__umariEventType` brand makes arrays like
 * `events: [ShopConnected, ShopReconnected]` produce a discriminated union
 * over `.type` in handler signatures.
 */
export interface EventDef<
  TType extends string = string,
  // `any` (rather than `object`) lets a tuple of specific event defs satisfy
  // `readonly EventDef[]` — otherwise the call signature `(data: TData) => …`
  // would make `EventDef<…, SpecificData, …>` *not* assignable to
  // `EventDef<…, object, …>` (function parameters are contravariant).
  TData = any,
  TDomainIds extends readonly string[] = readonly string[],
> extends EventTypeBrand<TType> {
  /** Event type string. */
  readonly type: TType;
  /** Declared domain-id field names. */
  readonly domainIds: TDomainIds;
  /** Phantom carrier for the payload type — never read at runtime. */
  readonly __umariData: TData;
  /** Construct an `EmittedEventPayload` from typed data. */
  (data: TData): EmittedEventPayload<TType>;
}

/**
 * `defineEvent<TData>()(type, options)` — define a typed event.
 *
 * The curried form lets you annotate `TData` while we infer `TType` and
 * `TDomainIds` from the runtime arguments.
 *
 * ```ts
 * type ShopConnectedData = { shopId: bigint; shopDomain: string };
 * const ShopConnected = defineEvent<ShopConnectedData>()('shop.connected', {
 *   domainIds: ['shopId'],
 * });
 * ```
 */
export function defineEvent<TData extends object>() {
  return function <
    const TType extends string,
    const TDomainIds extends readonly (keyof TData & string)[],
  >(
    type: TType,
    options: DefineEventOptions<TData, TDomainIds>,
  ): EventDef<TType, TData, TDomainIds> {
    const factory = ((data: TData): EmittedEventPayload<TType> => {
      const domainIds = new Map<string, string>();
      for (const field of options.domainIds) {
        const value = data[field];
        if (value === undefined || value === null) continue;
        domainIds.set(field, encode(value));
      }
      const encryptionScope = options.cryptoScope?.(data);
      return {
        __umariEventPayload: true,
        type,
        data,
        domainIds,
        encryptionScope,
      };
    }) as EventDef<TType, TData, TDomainIds>;
    // Attach metadata as own properties for runtime access.
    Object.defineProperties(factory, {
      type: { value: type, enumerable: true },
      domainIds: { value: options.domainIds, enumerable: true },
      __umariData: { value: undefined as unknown as TData, enumerable: false },
      __umariEventType: { value: type, enumerable: false },
    });
    return factory;
  };
}

function encode(value: unknown): string {
  if (typeof value === "string") return value;
  if (typeof value === "bigint") return value.toString();
  if (typeof value === "number") {
    if (!Number.isFinite(value)) {
      throw new Error(`domain id value must be finite, got ${value}`);
    }
    return value.toString();
  }
  if (typeof value === "boolean") return value ? "true" : "false";
  return String(value);
}

/** Extract the payload type from a `defineEvent` result. */
export type EventOf<E> = E extends EventDef<string, infer D, readonly string[]> ? D : never;

/** Extract the type-tag literal from a `defineEvent` result. */
export type EventTypeOf<E> = E extends EventDef<infer T, unknown, readonly string[]> ? T : never;

/** Build the discriminated `StoredEvent` shape for a `defineEvent` result. */
export type StoredEventOf<E> = E extends EventDef<infer T, infer D, readonly string[]>
  ? StoredEvent<D> & { readonly type: T; readonly data: D }
  : never;

/** Union of `StoredEvent`s for a tuple of `EventDef`s. */
export type StoredEventUnion<TList extends readonly EventDef[]> = {
  [K in keyof TList]: StoredEventOf<TList[K]>;
}[number];

/**
 * `emit(...events)` — collect zero or more emitted-event payloads from a
 * command's `execute` body.
 */
export function emit(...events: EmittedEventPayload[]): EmittedEventPayload[] {
  return events;
}

// Re-export so `stringifyPayload` shows up next to event types in the index.
export { stringifyPayload };
