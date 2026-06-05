// Effects can use the global `fetch` directly. StarlingMonkey wires it to
// `wasi:http/outgoing-handler` inside the WASM guest. In projector modules,
// `fetch` will fail: the projector WIT world does not import `wasi:http`.
//
// This file is documentation-as-code so users can write
// `import { fetch } from '@umari/js/http'` and have a single import path.

export const fetch: typeof globalThis.fetch = globalThis.fetch;
