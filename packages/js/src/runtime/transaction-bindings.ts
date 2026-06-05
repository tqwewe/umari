// Thin re-export of the WIT `transaction` resource so callers in this
// package import a single canonical path. The actual binding is resolved by
// jco at componentize time.

export { Transaction } from "umari:command/transaction@0.1.0";
