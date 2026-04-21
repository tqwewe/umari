#!/usr/bin/env node
// Copies WIT definitions into the package so they're included in the npm tarball.
// Runs via the `prepare` hook (before npm pack/publish and on local npm install).

import { cpSync, rmSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __dirname = dirname(fileURLToPath(import.meta.url));
const repoRoot = resolve(__dirname, "../../..");
const dest = resolve(__dirname, "../wit");

rmSync(dest, { recursive: true, force: true });

for (const type of ["command", "projector", "effect"]) {
  cpSync(resolve(repoRoot, "wit", type), resolve(dest, type), {
    recursive: true,
  });
}

console.log("copied wit/ into packages/js/wit/");
