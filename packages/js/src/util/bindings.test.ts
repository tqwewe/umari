import { describe, expect, it } from "vitest";
import {
  buildDcbQuery,
  extractBindings,
  matchesFoldQuery,
  type EventDomainId,
} from "./bindings.js";

describe("extractBindings", () => {
  it("stringifies bigint/number/boolean", () => {
    const b = extractBindings(["shopId", "active", "name"], {
      shopId: 42n,
      active: true,
      name: "demo",
    });
    expect(b.get("shopId")).toBe("42");
    expect(b.get("active")).toBe("true");
    expect(b.get("name")).toBe("demo");
  });

  it("skips undefined/null", () => {
    const b = extractBindings(["shopId", "planId"], { shopId: 1n, planId: undefined });
    expect(b.has("planId")).toBe(false);
  });
});

describe("buildDcbQuery", () => {
  it("groups entries with the same tag set", () => {
    const entries: EventDomainId[] = [
      {
        eventType: "shop.connected",
        dynamicFields: ["shopId"],
        staticFields: [],
      },
      {
        eventType: "shop.reconnected",
        dynamicFields: ["shopId"],
        staticFields: [],
      },
    ];
    const bindings = new Map([["shopId", "42"]]);
    const q = buildDcbQuery(entries, [bindings]);
    expect(q.items).toEqual([
      { types: ["shop.connected", "shop.reconnected"], tags: ["shopId:42"] },
    ]);
  });

  it("emits separate items when tags differ", () => {
    const entries: EventDomainId[] = [
      { eventType: "a", dynamicFields: ["x"], staticFields: [] },
      { eventType: "b", dynamicFields: ["y"], staticFields: [] },
    ];
    const b = new Map([
      ["x", "1"],
      ["y", "2"],
    ]);
    const q = buildDcbQuery(entries, [b]);
    expect(q.items).toEqual([
      { types: ["a"], tags: ["x:1"] },
      { types: ["b"], tags: ["y:2"] },
    ]);
  });

  it("static fields contribute tags", () => {
    const entries: EventDomainId[] = [
      {
        eventType: "webhook.received",
        dynamicFields: ["shopId"],
        staticFields: [["topic", "orders/paid"]],
      },
    ];
    const b = new Map([["shopId", "42"]]);
    const q = buildDcbQuery(entries, [b]);
    expect(q.items).toEqual([
      {
        types: ["webhook.received"],
        tags: ["shopId:42", "topic:orders/paid"].sort(),
      },
    ]);
  });

  it("handles multiple binding sets", () => {
    const entries: EventDomainId[] = [
      { eventType: "x", dynamicFields: ["shopId"], staticFields: [] },
    ];
    const sets = [new Map([["shopId", "1"]]), new Map([["shopId", "2"]])];
    const q = buildDcbQuery(entries, sets);
    expect(q.items).toEqual([
      { types: ["x"], tags: ["shopId:1"] },
      { types: ["x"], tags: ["shopId:2"] },
    ]);
  });
});

describe("matchesFoldQuery", () => {
  const entry: EventDomainId = {
    eventType: "x",
    dynamicFields: ["shopId"],
    staticFields: [],
  };

  it("returns true when bindings match the tag", () => {
    expect(matchesFoldQuery(entry, ["shopId:42"], new Map([["shopId", "42"]]))).toBe(true);
  });

  it("returns false when binding value differs from tag", () => {
    expect(matchesFoldQuery(entry, ["shopId:99"], new Map([["shopId", "42"]]))).toBe(false);
  });

  it("treats absent binding as 'any'", () => {
    expect(matchesFoldQuery(entry, ["shopId:42"], new Map())).toBe(true);
  });

  it("requires every static field to be present", () => {
    const e: EventDomainId = {
      eventType: "x",
      dynamicFields: [],
      staticFields: [["topic", "a"]],
    };
    expect(matchesFoldQuery(e, ["topic:a"], new Map())).toBe(true);
    expect(matchesFoldQuery(e, ["topic:b"], new Map())).toBe(false);
  });
});
