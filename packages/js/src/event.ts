import type { EmittedEvent, EventDef } from "./types.ts";

/**
 * Define a typed event with domain ID fields.
 *
 * Uses a curried call so TypeScript can infer TType and TDomainIds from the
 * arguments while TData is provided explicitly as the only type parameter:
 *
 *   const ShopConnected = defineEvent<{ shop_id: number; name: string }>()(
 *     "shop.connected",
 *     { domainIds: ["shop_id"] }
 *   );
 */
export function defineEvent<TData extends object>() {
  return function <
    TType extends string,
    const TDomainIds extends readonly string[],
  >(
    type: TType,
    options: { domainIds: TDomainIds },
  ): EventDef<TType, TData, TDomainIds> {
    return {
      type,
      domainIds: options.domainIds,
      create(data: TData): EmittedEvent {
        return {
          eventType: type,
          data: JSON.stringify(data),
          domainIds: options.domainIds.map((field) => {
            const id = (data as Record<string, unknown>)[field];
            return {
              name: field,
              id: id != null ? String(id) : null,
            };
          }),
        };
      },
    };
  };
}
