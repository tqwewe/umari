import { describe, expect, it } from "vitest";
import { eventIdFromCausation, IDEMPOTENCY_NAMESPACE } from "./idempotency.js";
import { sha1Sync } from "./uuid.js";

// Golden values produced by Rust `Uuid::new_v5(&IDEMPOTENCY_NAMESPACE, …)`.
// See `crates/umari/tests/cross_lang_goldens.rs`. Regenerate via:
//
//     cargo test -p umari --test cross_lang_goldens -- --nocapture
const goldens = [
  {
    correlation_id: "00000000-0000-0000-0000-000000000000",
    causation_id: "00000000-0000-0000-0000-000000000000",
    index: 0,
    expected: "bca2dea8-f2df-563c-8d43-f92a3f69548f",
  },
  {
    correlation_id: "11111111-1111-1111-1111-111111111111",
    causation_id: "22222222-2222-2222-2222-222222222222",
    index: 0,
    expected: "d8bee0ea-f637-579c-8285-93706e9e67d5",
  },
  {
    correlation_id: "11111111-1111-1111-1111-111111111111",
    causation_id: "22222222-2222-2222-2222-222222222222",
    index: 1,
    expected: "b43c197e-43ac-5d7a-90af-eb070797a174",
  },
  {
    correlation_id: "aabbccdd-eeff-1122-3344-556677889900",
    causation_id: "ffeeddcc-bbaa-9988-7766-554433221100",
    index: 7,
    expected: "cd266a3d-4aeb-5395-a076-f7da0e92c7b4",
  },
] as const;

describe("eventIdFromCausation matches Rust Uuid::new_v5(NAMESPACE, key)", () => {
  it("namespace constant matches", () => {
    expect(IDEMPOTENCY_NAMESPACE).toBe("e274f2bc-33c5-589f-8643-f3674d86773f");
  });

  for (const g of goldens) {
    it(`(${g.correlation_id}, ${g.causation_id}, ${g.index})`, () => {
      const id = eventIdFromCausation(g.correlation_id, g.causation_id, g.index);
      expect(id).toBe(g.expected);
    });
  }
});

describe("sha1Sync", () => {
  it("matches FIPS 180-1 sample 'abc'", () => {
    const out = sha1Sync(new TextEncoder().encode("abc"));
    const hex = [...out].map((b) => b.toString(16).padStart(2, "0")).join("");
    expect(hex).toBe("a9993e364706816aba3e25717850c26c9cd0d89d");
  });

  it("matches empty string sample", () => {
    const out = sha1Sync(new Uint8Array(0));
    const hex = [...out].map((b) => b.toString(16).padStart(2, "0")).join("");
    expect(hex).toBe("da39a3ee5e6b4b0d3255bfef95601890afd80709");
  });
});
