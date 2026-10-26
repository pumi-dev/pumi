// A repository to test against, made where the machine keeps temporary files: a real one, because
// the package's git work is the `git` binary and nothing here stands in for it.
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { dirname, join } from "node:path";

/** A directory of its own under the machine's temporary files. */
export function scratch(name: string): string {
  return mkdtempSync(join(tmpdir(), `pumi-git-${name}-`));
}

/** git in `dir`, for a test to set a repository up: it must not fail, so its exit code is checked
 * here. */
export function run(dir: string, ...args: string[]): string {
  const done = Bun.spawnSync(["git", ...args], { cwd: dir, stdout: "pipe", stderr: "pipe" });
  if (done.exitCode !== 0) throw new Error(`git ${args.join(" ")}: ${done.stderr.toString()}`);
  return done.stdout.toString();
}

/** `files`, written into `dir` (each under the directories its path names).
 */
export function write(dir: string, files: Record<string, string>): void {
  for (const [path, content] of Object.entries(files)) {
    const at = join(dir, path);
    mkdirSync(dirname(at), { recursive: true });
    writeFileSync(at, content);
  }
}

/** A repository with one commit, `files` in it, on branch `main`. */
export function repository(files: Record<string, string> = { "a.txt": "one\ntwo\n" }, name = "repo"): string {
  const dir = scratch(name);
  run(dir, "init", "--quiet", "--initial-branch", "main");
  run(dir, "config", "user.email", "test@pumi.dev");
  run(dir, "config", "user.name", "A Tester");
  write(dir, files);
  run(dir, "add", "--all");
  run(dir, "commit", "--quiet", "--message", "first");
  return dir;
}

/** A repository with nothing committed yet. */
export function empty(name = "empty"): string {
  const dir = scratch(name);
  run(dir, "init", "--quiet", "--initial-branch", "main");
  run(dir, "config", "user.email", "test@pumi.dev");
  run(dir, "config", "user.name", "A Tester");
  return dir;
}
