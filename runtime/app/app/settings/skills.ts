// Agents' hooks, as Settings → Agents shows them: `pumi skill list --json`, `add` and `remove`,
// run as the command line beside the app. Plain functions, so a test can read them.
import { existsSync } from "node:fs"
import { homedir } from "node:os"
import { delimiter, dirname, join } from "node:path"

/** An agent `pumi skill` knows, as `pumi skill list --json` describes it. */
export interface SkillAgent {
  id: string
  label: string
  /** Its program is on the PATH. */
  available: boolean
  hook: "installed" | "outdated" | "not_installed" | "unsupported"
  /** When it is told what pumi has for it: `prompt` (at start and each prompt), `start`, `none`. */
  reach: string
  /** What it takes besides hidden text: `queue`, `wake`. */
  accepts: string[]
  resumes: boolean
  /** The file its hook is in (`~/.claude/settings.json`). */
  file?: string
}

const exe = (name: string) => (process.platform === "win32" ? `${name}.exe` : name)

/** Where the command line is: `$PUMI_BIN`, the app bundle's (`<exe>/../Resources/bin`), this checkout's build, the PATH's. */
export function cliPath(env: Record<string, string | undefined> = process.env, execPath = process.execPath, has: (path: string) => boolean = existsSync): string | undefined {
  const candidates = [
    env.PUMI_BIN,
    join(dirname(execPath), "..", "Resources", "bin", exe("pumi")),
    join(dirname(execPath), exe("pumi-cli")),
    // `bun app/main.ts` in a checkout: its debug build.
    join(import.meta.dir, "..", "..", "..", "..", "target", "debug", exe("pumi-cli")),
  ]
  for (const path of candidates) if (path && has(path)) return path
  return Bun.which("pumi") ?? undefined
}

/**
 * The PATH the command line looks for agents in: the app's, after the directories agents are
 * installed to (an app started from the Dock has the system's PATH alone), as the app's own
 * (`src/bins.rs`) has it.
 */
export function agentPath(path = process.env.PATH ?? "", home = homedir()): string {
  const dirs = [join(home, ".local", "bin"), join(home, ".cargo", "bin"), join(home, ".bun", "bin"), "/opt/homebrew/bin", "/usr/local/bin"]
  const inherited = path.split(delimiter).filter(Boolean)
  return [...dirs.filter((d) => !inherited.includes(d)), ...inherited].join(delimiter)
}

/** Runs `pumi skill ARGS…`: its exit code and what it printed. */
export async function skill(args: string[]): Promise<{ code: number; out: string; err: string }> {
  const cli = cliPath()
  if (!cli) return { code: 127, out: "", err: "The pumi command line was not found beside the app or on the PATH." }
  const proc = Bun.spawn([cli, "skill", ...args], { env: { ...process.env, PATH: agentPath() }, stdin: "ignore", stdout: "pipe", stderr: "pipe" })
  const [out, err, code] = await Promise.all([new Response(proc.stdout).text(), new Response(proc.stderr).text(), proc.exited])
  return { code, out, err }
}

/** The agents of `pumi skill list --json`'s output; none when it is not that. */
export function parseList(out: string): SkillAgent[] {
  try {
    const agents = JSON.parse(out)?.agents
    return Array.isArray(agents) ? agents.filter((a) => a && typeof a.id === "string") : []
  } catch {
    return []
  }
}

/** Every agent, and its hook's state. */
export async function listAgents(): Promise<SkillAgent[]> {
  const { code, out, err } = await skill(["list", "--json"])
  if (code !== 0) throw new Error(messageOf(err.trim()) || `pumi skill list exited ${code}`)
  return parseList(out)
}

/** What `pumi skill add` and `remove` printed on stderr: the agents it could not do, one a line. */
async function change(verb: "add" | "remove", ids: string[]): Promise<void> {
  const { code, err } = await skill([verb, ...ids])
  const failed = err.trim()
  if (code !== 0 || failed) throw new Error(messageOf(failed) || `pumi skill ${verb} exited ${code}`)
}

/** A command line error's message: its JSON's (`{"kind", "message"}`), else the text as it is. */
export function messageOf(text: string): string {
  if (!text.startsWith("{")) return text
  try {
    const message = JSON.parse(text)?.message
    return typeof message === "string" ? message : text
  } catch {
    // Not the command line's JSON error after all: its text is the message.
    return text
  }
}

/** Installs (or brings up to date) the hooks of `ids`; none: of every agent on the PATH. */
export const installHooks = (ids: string[] = []) => change("add", ids)
export const removeHooks = (ids: string[]) => change("remove", ids)

/** What its row says of its hook, and the one thing its button does. */
export function hookState(a: SkillAgent): { text: string; action?: "install" | "update" } {
  switch (a.hook) {
    case "installed":
      return { text: "Installed" }
    case "outdated":
      return { text: "Outdated", action: "update" }
    case "not_installed":
      return { text: a.available ? "Not installed" : "Not found", action: "install" }
    default:
      return { text: "Unsupported" }
  }
}
