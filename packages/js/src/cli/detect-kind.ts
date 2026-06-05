// Cheap entry-file scan to pick the WIT world. We look for a call to one of
// `exportCommand` / `exportProjector` / `exportEffect` anywhere in the file
// using a regex (no TS AST required — keeps the CLI dependency-free).

import { readFileSync } from "node:fs";

export type ModuleKind = "command" | "projector" | "effect";

const PATTERNS: ReadonlyArray<[ModuleKind, RegExp]> = [
  ["command", /\bexportCommand\s*\(/],
  ["projector", /\bexportProjector\s*\(/],
  ["effect", /\bexportEffect\s*\(/],
];

export function detectKind(entryPath: string): ModuleKind {
  const source = readFileSync(entryPath, "utf8");
  // Strip block + line comments so commented-out calls don't false-positive.
  const stripped = source
    .replace(/\/\*[\s\S]*?\*\//g, "")
    .replace(/^\s*\/\/.*$/gm, "");

  const matches: ModuleKind[] = [];
  for (const [kind, pattern] of PATTERNS) {
    if (pattern.test(stripped)) matches.push(kind);
  }
  if (matches.length === 0) {
    throw new Error(
      `could not detect module kind in ${entryPath}: ` +
        `expected a call to exportCommand, exportProjector, or exportEffect.`,
    );
  }
  if (matches.length > 1) {
    throw new Error(
      `${entryPath} contains multiple exports (${matches.join(", ")}); ` +
        `split into separate modules.`,
    );
  }
  return matches[0]!;
}

/** Map module kind → WIT world name. Matches `crates/umari/wit/<kind>/world.wit`. */
export function worldNameFor(kind: ModuleKind): string {
  switch (kind) {
    case "command":
      return "command";
    case "projector":
      return "projector-world";
    case "effect":
      return "effect-world";
  }
}
