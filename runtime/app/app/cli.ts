// Whether the process's arguments are the command line (`pumi COMMAND …`, as in every tab) rather
// than the app: what `app/main.ts` and `app.tsx` decide by. A global flag of the CLI's
// (crates/pumi-cli/src/lib.rs) first is the command line too; `--safe` is the app's.
const CLI_FLAGS = ["--json", "--server", "--socket", "--headless", "--clean", "--version", "--help"]

export function isCli(argv: readonly string[]): boolean {
  const first = argv[0]
  if (!first) return false
  if (!first.startsWith("-")) return true
  return CLI_FLAGS.includes(first.split("=")[0]!)
}
