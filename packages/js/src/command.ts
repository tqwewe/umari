import type { EmittedEventPayload } from "./event.js";
import type { BoundFold } from "./fold.js";
import type { CommandContext } from "./util/lift-lower.js";

/** A schema-like object: anything with `.parse(unknown) → T`. */
export interface InputSchema<T> {
  parse(raw: unknown): T;
  /** Optional JSON Schema string, returned by `command.schema()`. */
  jsonSchema?(): string;
}

/** Map returned from `folds(input)`. Values are bound folds. */
export type FoldsMap = Readonly<Record<string, BoundFold<unknown>>>;

/** State map handed to `execute(...)`, keyed by the same names as `folds(input)`. */
export type FoldStates<TFolds extends FoldsMap> = {
  [K in keyof TFolds]: TFolds[K] extends BoundFold<infer S> ? S : never;
};

/** Argument object passed into `execute`. */
export interface ExecuteArgs<TInput, TFolds extends FoldsMap> {
  input: TInput;
  folds: FoldStates<TFolds>;
  context: CommandContext;
  /** Build an `EmittedEventPayload[]` from zero or more events. */
  emit: (...events: EmittedEventPayload[]) => EmittedEventPayload[];
  /** Short-circuit with `error::rejected(message)`. */
  reject: (message: string) => never;
  /** Short-circuit with `error::invalid-input(message)`. */
  invalidInput: (message: string) => never;
}

/** Options accepted by `defineCommand`. */
export interface DefineCommandOptions<TInput extends object, TFolds extends FoldsMap> {
  /** Optional input schema; if present, called with the parsed JSON. */
  input?: InputSchema<TInput>;
  /** Field names that key the command's DCB query. Must be in `TInput`. */
  domainIds: readonly (keyof TInput & string)[];
  /** Folds to replay before `execute`. Names become properties on `folds`. */
  folds: (input: TInput) => TFolds;
  /** Body — runs after all folds have been applied. */
  execute: (args: ExecuteArgs<TInput, TFolds>) => EmittedEventPayload[] | undefined;
}

/** Pure-data definition produced by `defineCommand`. */
export interface CommandDefinition<TInput extends object, TFolds extends FoldsMap> {
  readonly __umariCommand: true;
  readonly input: InputSchema<TInput> | undefined;
  readonly domainIds: readonly (keyof TInput & string)[];
  readonly folds: (input: TInput) => TFolds;
  readonly execute: (args: ExecuteArgs<TInput, TFolds>) => EmittedEventPayload[] | undefined;
}

/**
 * Define a command — pure data, no side effects. Wrap with `exportCommand`
 * to wire to the WIT exports.
 *
 * ```ts
 * export default defineCommand<Input>({
 *   domainIds: ['userId', 'projectId'] as const,
 *   folds: ({ userId, projectId }) => ({
 *     userExists: UserExistsFold({ userId }),
 *     project: EventFold(ProjectCreated)({ projectId, userId }),
 *   }),
 *   execute: ({ input, folds, emit, reject }) => {
 *     if (!folds.userExists) reject('user does not exist');
 *     if (folds.project.length > 0) return emit();
 *     return emit(ProjectCreated({ ... }));
 *   },
 * });
 * ```
 */
export function defineCommand<TInput extends object, TFolds extends FoldsMap>(
  opts: DefineCommandOptions<TInput, TFolds>,
): CommandDefinition<TInput, TFolds> {
  return {
    __umariCommand: true,
    input: opts.input,
    domainIds: opts.domainIds,
    folds: opts.folds,
    execute: opts.execute,
  };
}
