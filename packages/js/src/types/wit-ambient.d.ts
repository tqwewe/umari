// Ambient declarations for WIT-imported namespaces. esbuild treats these
// specifiers as `external`; jco resolves them at componentize time.
//
// This file MUST have no top-level imports/exports — TypeScript only treats
// `declare module "X" { … }` as a fresh ambient module when the containing
// file is a script. References below use inline shape types instead of
// importing from `./wit.ts`.

declare module "umari:common/types@0.1.0" {
  export interface EventFilter {
    types: string[];
    tags: string[];
  }
  export interface EventQuery {
    items: EventFilter[];
  }
  export interface StoredEvent {
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
}

declare module "umari:command/types@0.1.0" {
  export interface DomainId {
    name: string;
    id: string;
  }
  export interface CommandContext {
    correlationId: string;
    causationId: string;
    triggeringEventId?: string;
    idempotencyKey?: string;
  }
  export interface EmitEvent {
    id: string;
    eventType: string;
    data: string;
    domainIds: DomainId[];
    encryptionScope?: string;
  }
  export interface EmittedEvent {
    id: string;
    eventType: string;
    domainIds: DomainId[];
  }
  export interface ExecuteOutput {
    position?: bigint;
    events: EmittedEvent[];
  }
  export type Error =
    | { tag: "rejected"; val: string }
    | { tag: "invalid-input"; val: string };
}

declare module "umari:command/executor@0.1.0" {
  import type { CommandContext } from "umari:command/types@0.1.0";
  export function execute(command: string, input: string, context: CommandContext): void;
}

declare module "umari:command/transaction@0.1.0" {
  import type { CommandContext, EmitEvent } from "umari:command/types@0.1.0";
  import type { EventQuery, StoredEvent } from "umari:common/types@0.1.0";

  export class Transaction {
    constructor(query: EventQuery);
    nextBatch(): StoredEvent[];
    commit(context: CommandContext, events: EmitEvent[]): bigint | undefined;
  }
}

declare module "umari:sqlite/types@0.1.0" {
  export type Value =
    | { tag: "null" }
    | { tag: "integer"; val: bigint }
    | { tag: "real"; val: number }
    | { tag: "text"; val: string }
    | { tag: "blob"; val: Uint8Array };
  export interface Column {
    name: string;
    value: Value;
  }
  export interface Row {
    columns: Column[];
  }
  export type ConstraintViolationKind =
    | "unique"
    | "primary-key"
    | "not-null"
    | "foreign-key"
    | "check"
    | "other";
  export interface ConstraintViolation {
    kind: ConstraintViolationKind;
    message: string;
  }
  export type SqliteError = { tag: "constraint-violation"; val: ConstraintViolation };
}

declare module "umari:sqlite/connection@0.1.0" {
  import type { Row, Value } from "umari:sqlite/types@0.1.0";
  export function execute(sql: string, params: Value[]): bigint;
  export function executeBatch(sql: string): void;
  export function lastInsertRowid(): bigint | undefined;
  export function queryOne(sql: string, params: Value[]): Row;
  export function queryRow(sql: string, params: Value[]): Row | undefined;
}

declare module "umari:sqlite/statement@0.1.0" {
  import type { Row, Value } from "umari:sqlite/types@0.1.0";
  export class Stmt {
    constructor(sql: string);
    execute(params: Value[]): bigint;
    query(params: Value[]): Row[];
    queryOne(params: Value[]): Row;
    queryRow(params: Value[]): Row | undefined;
  }
}

declare module "umari:crypto/keys@0.1.0" {
  export function deleteKey(scope: string): void;
}
