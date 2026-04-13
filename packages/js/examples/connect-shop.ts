// Example: connect-shop command
import { z } from "zod";
import {
  defineCommand,
  defineEvent,
  defineFold,
  exportCommand,
} from "../src/index.ts";

// ─── Events ──────────────────────────────────────────────────────────────────

type ShopEventData = {
  shop_id: number;
  shop_domain: string;
  shop_name: string;
  access_token: string;
};

const ShopConnected = defineEvent<ShopEventData>()("shop.connected", {
  domainIds: ["shop_id"],
});

const ShopReconnected = defineEvent<ShopEventData>()("shop.reconnected", {
  domainIds: ["shop_id"],
});

// ─── Folds ────────────────────────────────────────────────────────────────────

const shopExists = defineFold({
  events: [ShopConnected, ShopReconnected],
  initial: false,
  apply: (_state, _event) => true,
});

// ─── Command ──────────────────────────────────────────────────────────────────

const ConnectShop = defineCommand({
  input: z.object({
    shop_id: z.number(),
    shop_domain: z.string().min(1),
    shop_name: z.string().min(1),
    access_token: z.string().min(1),
  }),
  domainIds: ["shop_id"],
  state: { exists: shopExists },
  emit({ exists }, input) {
    if (!exists) return [ShopConnected.create(input)];
    return [ShopReconnected.create(input)];
  },
});

// ─── WIT Exports ──────────────────────────────────────────────────────────────

export const { schema, query, execute } = exportCommand(ConnectShop);
