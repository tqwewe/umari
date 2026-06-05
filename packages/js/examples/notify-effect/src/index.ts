import { defineEffect, defineEvent, exportEffect, env } from "@umari/js";

type ShopConnectedData = {
  shopId: bigint;
  shopDomain: string;
};

const ShopConnected = defineEvent<ShopConnectedData>()("shop.connected", {
  domainIds: ["shopId"],
});

const NotifyOwner = defineEffect({
  events: [ShopConnected],
  init: () => ({ endpoint: env("NOTIFY_ENDPOINT", "https://example.com/notify") }),
  partitionKey: (event) => event.data.shopId.toString(),
  handle: async (event, state) => {
    const r = await fetch(state.endpoint, {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify({ shopId: event.data.shopId.toString() }),
    });
    if (!r.ok) throw new Error(`status ${r.status}`);
  },
});

export const { effect } = exportEffect(NotifyOwner);
