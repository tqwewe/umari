// Hand-written WIT shapes used internally by the runtime glue.
// Mirror crates/umari/wit/{common,command,projector,effect,sqlite,crypto}/.
// JCO's componentize step produces matching guest bindings at build time;
// these are for typing the imports we declare as `external` to esbuild.

// ───────────────────────── common ─────────────────────────

export interface WitEventFilter {
  types: string[];
  tags: string[];
}

export interface WitEventQuery {
  items: WitEventFilter[];
}

export interface WitStoredEvent {
  id: string;
  position: bigint;
  eventType: string;
  tags: string[];
  timestamp: bigint;
  correlationId: string;
  causationId: string;
  triggeringEventId?: string;
  idempotencyKey?: string;
  encryptionScope?: string;
  encryptionKeyId?: string;
  data: string;
}

// ───────────────────────── command ────────────────────────

export interface WitDomainId {
  name: string;
  id: string;
}

export interface WitCommandContext {
  correlationId: string;
  causationId: string;
  triggeringEventId?: string;
  idempotencyKey?: string;
}

export interface WitEmitEvent {
  id: string;
  eventType: string;
  data: string;
  domainIds: WitDomainId[];
  encryptionScope?: string;
}

export interface WitEmittedEvent {
  id: string;
  eventType: string;
  domainIds: WitDomainId[];
}

export interface WitExecuteOutput {
  position?: bigint;
  events: WitEmittedEvent[];
}

export type WitCommandError =
  | { tag: "rejected"; val: string }
  | { tag: "invalid-input"; val: string };

export type Result<T, E> = { tag: "ok"; val: T } | { tag: "err"; val: E };

// ───────────────────────── sqlite ─────────────────────────

export type WitSqliteValue =
  | { tag: "null" }
  | { tag: "integer"; val: bigint }
  | { tag: "real"; val: number }
  | { tag: "text"; val: string }
  | { tag: "blob"; val: Uint8Array };

export interface WitColumn {
  name: string;
  value: WitSqliteValue;
}

export interface WitRow {
  columns: WitColumn[];
}

export type WitConstraintViolationKind =
  | "unique"
  | "primary-key"
  | "not-null"
  | "foreign-key"
  | "check"
  | "other";

export interface WitConstraintViolation {
  kind: WitConstraintViolationKind;
  message: string;
}

export type WitSqliteError = { tag: "constraint-violation"; val: WitConstraintViolation };

// Ambient module declarations for the WIT-imported namespaces live in
// `src/types/wit-ambient.d.ts` so TS treats them as fresh ambient modules
// rather than augmentations.
