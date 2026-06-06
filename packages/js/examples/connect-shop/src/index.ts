import {
  defineCommand,
  defineEvent,
  defineFold,
  exportCommand,
} from "@umari/js";

type ShopConnectedData = {
  shopId: bigint;
  shopDomain: string;
  accessToken: string;
};

export const ShopConnected = defineEvent<ShopConnectedData>()("shop.connected", {
  domainIds: ["shopId"],
});

const ShopExistsFold = defineFold({
  domainIds: ["shopId"] as const,
  events: [ShopConnected],
  initial: () => false,
  apply: (_state, _event) => true,
});

type Input = {
  shopId: bigint;
  shopDomain: string;
  accessToken: string;
};

const ConnectShop = defineCommand<Input, { exists: ReturnType<typeof ShopExistsFold> }>({
  domainIds: ["shopId"] as const,
  folds: ({ shopId }) => ({
    exists: ShopExistsFold({ shopId }),
  }),
  execute: ({ input, folds, emit }) => {
    if (folds.exists) return emit(); // idempotent — already connected
    return emit(
      ShopConnected({
        shopId: input.shopId,
        shopDomain: input.shopDomain,
        accessToken: input.accessToken,
      }),
    );
  },
});

export const { schema, execute } = exportCommand(ConnectShop);
