// http.ts — HTTP wrapper for effects
//
// ComponentizeJS backed by WASI P2 provides `globalThis.fetch` automatically
// when the WIT world imports `wasi:http/outgoing-handler`. No explicit wrapper
// is needed — simply re-export the global fetch for use in EffectContext.
//
// If ComponentizeJS does NOT polyfill globalThis.fetch in a future version,
// replace this with an explicit wrapper over the jco-generated WASI HTTP bindings.

// ComponentizeJS provides globalThis.fetch backed by wasi:http/outgoing-handler
export const fetch = globalThis.fetch as (
  input: string | URL | Request,
  init?: RequestInit,
) => Promise<Response>;
