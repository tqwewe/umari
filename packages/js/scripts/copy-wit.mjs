#!/usr/bin/env node
// Mirrors the WIT subtree from crates/umari/wit/ into packages/js/wit/.
// Runs as `prepare` so it works both in this monorepo (dev) and inside a
// published tarball (where wit/ ships in the tarball already and the source
// directory does not exist — in that case we no-op).

import { existsSync, mkdirSync, readdirSync, statSync, copyFileSync, rmSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PKG_ROOT = resolve(__dirname, "..");
const SRC_ROOT = resolve(PKG_ROOT, "..", "..", "crates", "umari", "wit");
const DEST_ROOT = resolve(PKG_ROOT, "wit");

const KINDS = ["command", "projector", "effect"];

if (!existsSync(SRC_ROOT)) {
  // Running inside a published tarball with wit/ already present. Nothing to do.
  process.exit(0);
}

function copyDir(src, dest) {
  mkdirSync(dest, { recursive: true });
  for (const entry of readdirSync(src)) {
    const srcPath = join(src, entry);
    const destPath = join(dest, entry);
    const st = statSync(srcPath);
    if (st.isDirectory()) {
      copyDir(srcPath, destPath);
    } else {
      copyFileSync(srcPath, destPath);
    }
  }
}

if (existsSync(DEST_ROOT)) {
  rmSync(DEST_ROOT, { recursive: true, force: true });
}

for (const kind of KINDS) {
  const src = join(SRC_ROOT, kind);
  const dest = join(DEST_ROOT, kind);
  if (!existsSync(src)) {
    console.error(`copy-wit: missing ${src}`);
    process.exit(1);
  }
  copyDir(src, dest);
}

console.log(`copy-wit: mirrored ${KINDS.join(", ")} → ${DEST_ROOT}`);
