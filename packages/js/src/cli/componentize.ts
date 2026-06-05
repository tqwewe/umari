import { spawn } from "node:child_process";
import { existsSync } from "node:fs";
import { resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";

const __filename = fileURLToPath(import.meta.url);
const __dirname = dirname(__filename);

export interface ComponentizeOptions {
  /** Path to the esbuild output JS bundle. */
  bundle: string;
  /** Directory containing the per-kind WIT subtree. */
  witDir: string;
  /** WIT world name to target. */
  worldName: string;
  /** Output .wasm path. */
  out: string;
  /** Extra `--enable` features (e.g. `'http'`, `'clocks'`). */
  enable?: string[];
  /** Print the underlying command before running. */
  verbose?: boolean;
}

/** Run `jco componentize` with the given options. */
export async function componentize(opts: ComponentizeOptions): Promise<void> {
  const jcoBin = findJco();
  const args: string[] = [
    "componentize",
    opts.bundle,
    "--wit",
    opts.witDir,
    "--world-name",
    opts.worldName,
    "--out",
    opts.out,
  ];
  for (const feature of opts.enable ?? []) {
    args.push("--enable", feature);
  }
  if (opts.verbose) {
    process.stderr.write(`+ ${jcoBin} ${args.join(" ")}\n`);
  }
  await new Promise<void>((resolveP, rejectP) => {
    const proc = spawn(jcoBin, args, { stdio: "inherit" });
    proc.on("error", rejectP);
    proc.on("exit", (code) => {
      if (code === 0) resolveP();
      else rejectP(new Error(`jco componentize exited with code ${code}`));
    });
  });
}

function findJco(): string {
  // 1. Local node_modules/.bin
  const candidates = [
    resolve(process.cwd(), "node_modules/.bin/jco"),
    resolve(__dirname, "../../../node_modules/.bin/jco"),
  ];
  for (const c of candidates) {
    if (existsSync(c)) return c;
  }
  // 2. PATH lookup
  return "jco";
}
