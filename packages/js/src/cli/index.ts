import { build } from "./build.js";

const USAGE = `Usage:
  umari-js build [entry.ts] [--out dist/module.wasm] [--minify]
  umari-js --help

If no entry is provided, defaults to src/index.ts. The module kind (command,
projector, effect) is inferred from the entry file's exports.
`;

async function main(): Promise<number> {
  const argv = process.argv.slice(2);
  const cmd = argv[0];
  if (!cmd || cmd === "--help" || cmd === "-h") {
    process.stdout.write(USAGE);
    return 0;
  }

  if (cmd === "build") {
    const positional: string[] = [];
    const flags: Record<string, string | boolean> = {};
    for (let i = 1; i < argv.length; i++) {
      const a = argv[i]!;
      if (a === "--out") {
        flags.out = argv[++i] ?? "";
      } else if (a === "--minify") {
        flags.minify = true;
      } else if (a.startsWith("--")) {
        const [k, ...rest] = a.slice(2).split("=");
        flags[k!] = rest.length ? rest.join("=") : true;
      } else {
        positional.push(a);
      }
    }
    const entry = positional[0] ?? "src/index.ts";
    const out = (flags.out as string | undefined) ?? "dist/module.wasm";
    const minify = flags.minify === true;
    await build({ entry, out, minify });
    return 0;
  }

  process.stderr.write(`unknown subcommand: ${cmd}\n${USAGE}`);
  return 1;
}

main().then(
  (code) => process.exit(code),
  (err) => {
    console.error(err);
    process.exit(1);
  },
);
