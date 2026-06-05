import { describe, expect, it } from "vitest";
import { parsePayload, stringifyPayload } from "./bigint-json.js";

describe("bigint-json", () => {
  it("serialises bigint as string", () => {
    const s = stringifyPayload({ shopId: 9007199254740993n, name: "demo" });
    expect(s).toBe('{"shopId":"9007199254740993","name":"demo"}');
  });

  it("does not auto-revive — payload keeps strings as strings", () => {
    const obj = parsePayload<{ shopId: string; name: string }>(
      '{"shopId":"42","name":"x"}',
    );
    expect(obj.shopId).toBe("42");
    expect(typeof obj.shopId).toBe("string");
  });

  it("round-trips with user-side BigInt coercion", () => {
    const original = { shopId: 42n, name: "x" };
    const round = parsePayload<{ shopId: string; name: string }>(
      stringifyPayload(original),
    );
    expect(BigInt(round.shopId)).toBe(original.shopId);
  });
});
