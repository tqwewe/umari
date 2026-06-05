import {
  defineEvent,
  defineProjector,
  exportProjector,
  sqlite,
} from "@umari/js";

type ShopConnectedData = {
  shopId: bigint;
  shopDomain: string;
  accessToken: string;
};

const ShopConnected = defineEvent<ShopConnectedData>()("shop.connected", {
  domainIds: ["shopId"],
});

const Shops = defineProjector({
  events: [ShopConnected],
  init: () => {
    sqlite.executeBatch(
      `CREATE TABLE IF NOT EXISTS shops (
         shop_id TEXT PRIMARY KEY,
         shop_domain TEXT NOT NULL,
         access_token TEXT NOT NULL
       );`,
    );
  },
  handle: (event) => {
    switch (event.type) {
      case "shop.connected":
        sqlite.execute(
          "INSERT INTO shops (shop_id, shop_domain, access_token) VALUES (?, ?, ?)",
          [event.data.shopId, event.data.shopDomain, event.data.accessToken],
        );
        break;
    }
  },
});

export const { projector } = exportProjector(Shops);
