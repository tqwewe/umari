import type {
  FoldDef,
  MultiFoldRule,
  RuleDef,
  SingleFoldRule,
  StateOf,
  StatesOf,
} from "./types.ts";

// Overload 1: single fold
export function defineRule<
  TFold extends FoldDef<any, any>,
  TInput extends object,
>(
  fold: TFold,
  check: (
    state: StateOf<TFold>,
    input: TInput,
  ) => string | null | undefined | void,
): SingleFoldRule<TFold, TInput>;

// Overload 2: named object of folds
export function defineRule<
  TFolds extends Record<string, FoldDef<any, any>>,
  TInput extends object,
>(
  folds: TFolds,
  check: (
    states: StatesOf<TFolds>,
    input: TInput,
  ) => string | null | undefined | void,
): MultiFoldRule<TFolds, TInput>;

export function defineRule(foldsOrFold: any, check: any): RuleDef {
  if (
    typeof foldsOrFold === "object" &&
    foldsOrFold !== null &&
    foldsOrFold._tag === "fold"
  ) {
    return { _tag: "rule", _kind: "single", _fold: foldsOrFold, _check: check };
  }
  return { _tag: "rule", _kind: "multi", _folds: foldsOrFold, _check: check };
}
