// Minimal UUID v4 + v5 implementation. v5 requires SHA-1; we use
// `globalThis.crypto.subtle.digest('SHA-1', …)` when available (Node ≥ 20,
// StarlingMonkey ≥ 0.10) and fall back to a hand-rolled SHA-1 otherwise.

const HEX = "0123456789abcdef";

export type Uuid = string;

export function uuidToBytes(uuid: Uuid): Uint8Array {
  const hex = uuid.replace(/-/g, "").toLowerCase();
  if (hex.length !== 32) {
    throw new Error(`invalid uuid: ${uuid}`);
  }
  const out = new Uint8Array(16);
  for (let i = 0; i < 16; i++) {
    const c0 = HEX.indexOf(hex[i * 2]!);
    const c1 = HEX.indexOf(hex[i * 2 + 1]!);
    if (c0 < 0 || c1 < 0) {
      throw new Error(`invalid uuid: ${uuid}`);
    }
    out[i] = (c0 << 4) | c1;
  }
  return out;
}

export function bytesToUuid(b: Uint8Array): Uuid {
  if (b.length !== 16) {
    throw new Error("uuid must be 16 bytes");
  }
  const h = (i: number) => HEX[b[i]! >> 4]! + HEX[b[i]! & 0xf]!;
  return (
    h(0) + h(1) + h(2) + h(3) + "-" +
    h(4) + h(5) + "-" +
    h(6) + h(7) + "-" +
    h(8) + h(9) + "-" +
    h(10) + h(11) + h(12) + h(13) + h(14) + h(15)
  );
}

/** RFC 4122 v4 (random). Uses `crypto.getRandomValues`. */
export function uuidv4(): Uuid {
  const b = new Uint8Array(16);
  globalThis.crypto.getRandomValues(b);
  b[6] = (b[6]! & 0x0f) | 0x40;
  b[8] = (b[8]! & 0x3f) | 0x80;
  return bytesToUuid(b);
}

/**
 * RFC 4122 v5 (SHA-1 namespaced). Mirrors `Uuid::new_v5(&namespace, name)`
 * from the `uuid` crate.
 */
export async function uuidv5(namespace: Uuid, name: Uint8Array): Promise<Uuid> {
  const ns = uuidToBytes(namespace);
  const buf = new Uint8Array(ns.length + name.length);
  buf.set(ns, 0);
  buf.set(name, ns.length);
  const hash = await sha1(buf);
  const b = hash.slice(0, 16);
  b[6] = (b[6]! & 0x0f) | 0x50; // version 5
  b[8] = (b[8]! & 0x3f) | 0x80; // RFC 4122 variant
  return bytesToUuid(b);
}

/** Synchronous v5 — uses the pure-JS SHA-1 fallback. */
export function uuidv5Sync(namespace: Uuid, name: Uint8Array): Uuid {
  const ns = uuidToBytes(namespace);
  const buf = new Uint8Array(ns.length + name.length);
  buf.set(ns, 0);
  buf.set(name, ns.length);
  const hash = sha1Sync(buf);
  const b = hash.slice(0, 16);
  b[6] = (b[6]! & 0x0f) | 0x50;
  b[8] = (b[8]! & 0x3f) | 0x80;
  return bytesToUuid(b);
}

// ───────────────────────── SHA-1 ─────────────────────────

async function sha1(data: Uint8Array): Promise<Uint8Array> {
  const subtle = (globalThis as { crypto?: { subtle?: SubtleCrypto } }).crypto?.subtle;
  if (subtle && typeof subtle.digest === "function") {
    const ab = await subtle.digest("SHA-1", data);
    return new Uint8Array(ab);
  }
  return sha1Sync(data);
}

/**
 * Pure-JS SHA-1 — used inside WASM guests where `crypto.subtle` may be
 * missing. Adapted from FIPS 180-4. Returns 20 raw bytes.
 */
export function sha1Sync(data: Uint8Array): Uint8Array {
  const bitLen = BigInt(data.length) * 8n;

  // Pad: append 0x80, then zeros, then 64-bit big-endian length.
  const padLen = data.length + 1 + ((56 - ((data.length + 1) % 64) + 64) % 64) + 8;
  const padded = new Uint8Array(padLen);
  padded.set(data, 0);
  padded[data.length] = 0x80;
  const view = new DataView(padded.buffer);
  view.setUint32(padLen - 8, Number(bitLen >> 32n) >>> 0, false);
  view.setUint32(padLen - 4, Number(bitLen & 0xffffffffn) >>> 0, false);

  let h0 = 0x67452301;
  let h1 = 0xefcdab89;
  let h2 = 0x98badcfe;
  let h3 = 0x10325476;
  let h4 = 0xc3d2e1f0;

  const w = new Uint32Array(80);
  for (let i = 0; i < padLen; i += 64) {
    for (let t = 0; t < 16; t++) {
      w[t] = view.getUint32(i + t * 4, false);
    }
    for (let t = 16; t < 80; t++) {
      const x = w[t - 3]! ^ w[t - 8]! ^ w[t - 14]! ^ w[t - 16]!;
      w[t] = ((x << 1) | (x >>> 31)) >>> 0;
    }
    let a = h0;
    let b = h1;
    let c = h2;
    let d = h3;
    let e = h4;
    for (let t = 0; t < 80; t++) {
      let f: number;
      let k: number;
      if (t < 20) {
        f = (b & c) | (~b & d);
        k = 0x5a827999;
      } else if (t < 40) {
        f = b ^ c ^ d;
        k = 0x6ed9eba1;
      } else if (t < 60) {
        f = (b & c) | (b & d) | (c & d);
        k = 0x8f1bbcdc;
      } else {
        f = b ^ c ^ d;
        k = 0xca62c1d6;
      }
      const temp = (((a << 5) | (a >>> 27)) + f + e + k + w[t]!) >>> 0;
      e = d;
      d = c;
      c = ((b << 30) | (b >>> 2)) >>> 0;
      b = a;
      a = temp;
    }
    h0 = (h0 + a) >>> 0;
    h1 = (h1 + b) >>> 0;
    h2 = (h2 + c) >>> 0;
    h3 = (h3 + d) >>> 0;
    h4 = (h4 + e) >>> 0;
  }

  const out = new Uint8Array(20);
  const outView = new DataView(out.buffer);
  outView.setUint32(0, h0, false);
  outView.setUint32(4, h1, false);
  outView.setUint32(8, h2, false);
  outView.setUint32(12, h3, false);
  outView.setUint32(16, h4, false);
  return out;
}
