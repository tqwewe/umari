/**
 * Envelope wrapped around every event read from the event store. Mirrors
 * Rust `StoredEvent<T>` in `crates/umari/src/event.rs`.
 *
 * `type` is the event-type string (e.g. `"user.registered"`). When `events`
 * on a fold/projector/effect contains multiple event definitions, this field
 * is the discriminator narrowing TS into the right `data` shape.
 */
export interface StoredEvent<TData = unknown> {
  readonly id: string;
  readonly position: bigint;
  readonly type: string;
  readonly tags: readonly string[];
  readonly timestamp: Date;
  readonly correlationId: string;
  readonly causationId: string;
  readonly triggeringEventId?: string;
  readonly idempotencyKey?: string;
  readonly encryptionScope?: string;
  readonly encryptionKeyId?: string;
  readonly data: TData;
}

/**
 * Dispatch a stored event to a per-type handler. Throws on unknown types so
 * unhandled events are caught at runtime rather than silently dropped.
 *
 * ```ts
 * matchEvent(event, {
 *   'user.registered': (e) => sqlite.execute(...),
 *   'user.reactivated': (e) => sqlite.execute(...),
 * });
 * ```
 */
export function matchEvent<
  E extends StoredEvent<unknown> & { readonly type: string },
  R,
>(
  event: E,
  handlers: { [K in E["type"]]: (event: Extract<E, { type: K }>) => R },
): R {
  const handler = handlers[event.type as E["type"]];
  if (!handler) {
    throw new Error(`matchEvent: no handler for ${event.type}`);
  }
  return handler(event as Extract<E, { type: E["type"] }>);
}
