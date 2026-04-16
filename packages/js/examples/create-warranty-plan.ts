// Example: create-warranty-plan command (with rules)
import { z } from "zod";
import {
  defineCommand,
  defineEvent,
  defineFold,
  defineRule,
  exportCommand,
  scopedEvent,
} from "../src/index.ts";

// ─── Events ──────────────────────────────────────────────────────────────────

type ShopConnectedData = {
  shop_id: number;
  shop_domain: string;
  shop_name: string;
  access_token: string;
};

type ShopDisconnectedData = {
  shop_id: number;
};

type WarrantyPlanCreatedData = {
  shop_id: number;
  plan_id: string;
  name: string;
  duration_months: number;
  price: number;
};

const ShopConnected = defineEvent<ShopConnectedData>()("shop.connected", {
  domainIds: ["shop_id"],
});

const ShopDisconnected = defineEvent<ShopDisconnectedData>()(
  "shop.disconnected",
  { domainIds: ["shop_id"] },
);

const WarrantyPlanCreated = defineEvent<WarrantyPlanCreatedData>()(
  "warranty_plan.created",
  { domainIds: ["shop_id", "plan_id"] },
);

// ─── Folds ────────────────────────────────────────────────────────────────────

// Tracks whether the shop is currently connected.
const shopConnectionState = defineFold({
  events: [ShopConnected, ShopDisconnected],
  initial: { connected: false },
  apply: (_state, event) => {
    if (event.type === "shop.connected") {
      return { connected: true };
    }
    return { connected: false };
  },
});

// Tracks all plan names created for a shop (keyed by plan_id).
// Scoped to shop_id so it loads all plans for the shop, not just the specific plan_id.
const planNames = defineFold({
  events: [scopedEvent(WarrantyPlanCreated, ["shop_id"])],
  initial: {} as Record<string, string>,
  apply: (state, event) => ({
    ...state,
    [event.data.plan_id]: event.data.name,
  }),
});

// ─── Rules ────────────────────────────────────────────────────────────────────

// Shop must be connected before creating a plan.
const shopIsConnected = defineRule(shopConnectionState, (state) => {
  if (!state.connected) return "shop is not connected";
});

// Plan name must be unique within the shop.
const planNameIsUnique = defineRule(
  planNames,
  (state, input: { name: string }) => {
    const taken = Object.values(state).some(
      (name) => name.toLowerCase() === input.name.toLowerCase(),
    );
    if (taken) return "plan name already exists";
  },
);

// ─── Command ──────────────────────────────────────────────────────────────────

const CreateWarrantyPlan = defineCommand({
  input: z.object({
    shop_id: z.number(),
    plan_id: z.string().uuid(),
    name: z.string().min(3).max(64),
    duration_months: z.number().int().min(1).max(60),
    price: z.number().nonnegative(),
  }),
  domainIds: ["shop_id", "plan_id"],
  rules: [shopIsConnected, planNameIsUnique],
  emit(_state, input) {
    return [WarrantyPlanCreated.create(input)];
  },
});

// ─── WIT Exports ──────────────────────────────────────────────────────────────

export const { schema, query, execute } = exportCommand(CreateWarrantyPlan);
