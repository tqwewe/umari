/**
 * Command error classification, mirroring the Rust `ErrorCode` enum and the
 * `error` variant on the `umari:command` WIT world.
 *
 * - `rejected` — the command violated a business rule (e.g. invariant check
 *   inside `execute` failed).
 * - `invalid-input` — the caller supplied a malformed input.
 * - `internal` — any other error. Returned as `rejected` over the wire today
 *   (the WIT world only has two variants), but kept distinct here so callers
 *   can tell intent.
 */
export const ErrorCode = {
  Rejected: "rejected",
  InvalidInput: "invalid-input",
  Internal: "internal",
} as const;
export type ErrorCode = (typeof ErrorCode)[keyof typeof ErrorCode];

/** Thrown to short-circuit a command with `error::rejected(msg)`. */
export class RejectError extends Error {
  readonly code: typeof ErrorCode.Rejected = ErrorCode.Rejected;
  constructor(message: string) {
    super(message);
    this.name = "RejectError";
  }
}

/** Thrown to short-circuit a command with `error::invalid-input(msg)`. */
export class InvalidInputError extends Error {
  readonly code: typeof ErrorCode.InvalidInput = ErrorCode.InvalidInput;
  constructor(message: string) {
    super(message);
    this.name = "InvalidInputError";
  }
}

/** Throws a `RejectError`. Sugar that reads naturally inside `execute`. */
export function reject(message: string): never {
  throw new RejectError(message);
}

/** Throws an `InvalidInputError`. */
export function invalidInput(message: string): never {
  throw new InvalidInputError(message);
}
