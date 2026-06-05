// Public API of `@umari/js`.

// ── Events
export {
  defineEvent,
  emit,
  type EventDef,
  type EventOf,
  type EventTypeOf,
  type StoredEventOf,
  type StoredEventUnion,
  type EmittedEventPayload,
} from "./event.js";

// ── Stored events
export { matchEvent, type StoredEvent } from "./stored-event.js";

// ── Folds
export {
  defineFold,
  EventFold,
  LatestEvent,
  EventCounter,
  EventToggle,
  type FoldDef,
  type BoundFold,
  type StateOf,
} from "./fold.js";

// ── Module kinds
export { defineCommand, type CommandDefinition, type InputSchema, type ExecuteArgs } from "./command.js";
export { defineProjector, type ProjectorDefinition } from "./projector.js";
export { defineEffect, type EffectDefinition } from "./effect.js";

// ── WIT-export wrappers
export { exportCommand, type CommandExports } from "./runtime/command-exports.js";
export { exportProjector, type ProjectorResource } from "./runtime/projector-exports.js";
export { exportEffect, type EffectResource } from "./runtime/effect-exports.js";

// ── FoldQuery
export { foldQuery, type FoldQueryResult } from "./fold-query.js";

// ── Cross-module execution
export { execute } from "./runtime/executor-bindings.js";

// ── Errors
export {
  RejectError,
  InvalidInputError,
  ErrorCode,
  reject,
  invalidInput,
} from "./errors.js";

// ── Host bindings (re-exported here for convenience; also available as
//    `@umari/js/sqlite` etc.)
export * as sqlite from "./sqlite.js";
export { Row, PreparedStatement, type SqliteParam, type SqliteError } from "./sqlite.js";
export { deleteCryptoKey } from "./crypto.js";
export { env, envOptional } from "./env.js";

// ── Context shape
export type { CommandContext } from "./util/lift-lower.js";
