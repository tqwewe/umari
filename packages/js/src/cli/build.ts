import { existsSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { build as esbuild } from "esbuild";
import { detectKind, worldNameFor, type ModuleKind } from "./detect-kind.js";
import { componentize } from "./componentize.js";

export interface BuildOptions {
  entry: string;
  out: string;
  minify?: boolean;
}

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

const PKG_ROOT = resolve(__dirname, "../..");

/** Build a Umari WASM module from a single TS/JS entry. */
export async function build(opts: BuildOptions): Promise<void> {
  const entry = resolve(process.cwd(), opts.entry);
  const outWasm = resolve(process.cwd(), opts.out);

  if (!existsSync(entry)) {
    throw new Error(`entry not found: ${entry}`);
  }

  const kind = detectKind(entry);
  const world = worldNameFor(kind);

  const outDir = dirname(outWasm);
  mkdirSync(outDir, { recursive: true });
  const bundlePath = resolve(outDir, "bundle.js");

  process.stderr.write(`umari-js build: ${kind} → ${outWasm}\n`);
  process.stderr.write(`  esbuild ${entry} → ${bundlePath}\n`);

  await esbuild({
    entryPoints: [entry],
    bundle: true,
    format: "esm",
    platform: "neutral",
    target: ["es2022"],
    outfile: bundlePath,
    minify: opts.minify ?? false,
    sourcemap: false,
    external: [
      // WIT-imported modules: passed through to jco unresolved.
      "umari:*",
      "wasi:*",
    ],
  });

  const witDir = findWitDir(kind);
  process.stderr.write(`  jco componentize → ${outWasm} (world=${world}, wit=${witDir})\n`);

  await componentize({
    bundle: bundlePath,
    witDir,
    worldName: world,
    out: outWasm,
    enable: enableFor(kind),
    verbose: false,
  });

  process.stderr.write(`umari-js build: done\n`);
}

function enableFor(kind: ModuleKind): string[] {
  switch (kind) {
    case "command":
      return [];
    case "projector":
      return [];
    case "effect":
      // wasi:http + wasi:clocks come into the effect world. Both are also
      // wasi p2 standard imports — jco's `--enable` flag toggles inclusion
      // of the corresponding StarlingMonkey wrappers in the guest bundle.
      return ["http", "clocks"];
  }
}

/**
 * Find `wit/<kind>/`. Looks in the user's project node_modules first, then
 * the `@umari/js` package itself (for `npx umari-js build` directly from the
 * monorepo).
 */
function findWitDir(kind: ModuleKind): string {
  const cwd = process.cwd();
  const candidates = [
    resolve(cwd, "node_modules/@umari/js/wit", kind),
    resolve(PKG_ROOT, "wit", kind),
  ];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  throw new Error(
    `cannot find @umari/js WIT directory for ${kind}. ` +
      `Looked in: ${candidates.join(", ")}`,
  );
}
