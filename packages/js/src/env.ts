// Read an environment variable. StarlingMonkey exposes `process.env`-style
// globals via `wasi:cli/environment`. We use the standard `process` shape so
// the same code runs under Node for unit tests.

/** Read an env var. Throws if missing. */
export function env(name: string): string;
/** Read an env var, returning the default if missing. */
export function env(name: string, defaultValue: string): string;
export function env(name: string, defaultValue?: string): string {
  const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  const value = proc?.env?.[name];
  if (value !== undefined) return value;
  if (defaultValue !== undefined) return defaultValue;
  throw new Error(`missing env var: ${name}`);
}

/** Read an optional env var. Returns `undefined` if missing. */
export function envOptional(name: string): string | undefined {
  const proc = (globalThis as { process?: { env?: Record<string, string | undefined> } }).process;
  return proc?.env?.[name];
}
