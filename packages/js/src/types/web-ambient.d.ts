// Minimal ambient declarations for Web Crypto + fetch. Both are present in
// Node ≥ 20 and StarlingMonkey, but our `lib` excludes DOM/WebWorker.

interface SubtleCrypto {
  digest(algorithm: string | { name: string }, data: ArrayBuffer | ArrayBufferView): Promise<ArrayBuffer>;
}

interface Crypto {
  getRandomValues<T extends ArrayBufferView | null>(array: T): T;
  randomUUID?(): string;
  readonly subtle: SubtleCrypto;
}

declare var crypto: Crypto;

// fetch is provided by StarlingMonkey (wasi:http) inside effect modules and
// by Node natively. We only need the minimal shape used in docs/examples.
interface RequestInit {
  method?: string;
  headers?: Record<string, string>;
  body?: string | Uint8Array | null;
}

interface Response {
  readonly status: number;
  readonly ok: boolean;
  readonly headers: { get(name: string): string | null };
  text(): Promise<string>;
  json(): Promise<unknown>;
  arrayBuffer(): Promise<ArrayBuffer>;
}

declare function fetch(input: string | URL, init?: RequestInit): Promise<Response>;

declare class URL {
  constructor(input: string, base?: string | URL);
  href: string;
  toString(): string;
}
