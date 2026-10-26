// The repository as it changes: a watch on the work tree (and the repository's own directory, which
// is where a commit or a checkout shows), called at most once a burst, and never for the paths that
// can only be git writing down something it is about to write again.
import { watch as fsWatch, type FSWatcher } from "node:fs";
import { join } from "node:path";

/** How long after the first change of a burst the page looks again, in ms. */
export const DEBOUNCE = 300;

/**
 * Calls `onChange` after a burst of writes anywhere under `root`. Gives back what stops it.
 *
 * The whole tree is watched where the platform can (macOS), and the work tree's root with the
 * repository's own directory where it cannot (Linux has no recursive watch): a status sees the rest.
 */
export function watch(root: string, onChange: (paths: string[]) => void, ms: number = DEBOUNCE): () => void {
  const watchers: FSWatcher[] = [];
  const pending = new Set<string>();
  let timer: ReturnType<typeof setTimeout> | null = null;
  const schedule = () => {
    if (timer !== null) return;
    timer = setTimeout(() => {
      timer = null;
      const paths = [...pending];
      pending.clear();
      onChange(paths);
    }, ms);
  };
  const heard = (_event: string, filename: string | null) => {
    pending.add(filename === null ? "" : String(filename));
    schedule();
  };
  const listen = (dir: string, recursive: boolean): boolean => {
    try {
      watchers.push(fsWatch(dir, { recursive }, heard));
      return true;
    } catch {
      return false;
    }
  };
  if (!listen(root, true)) {
    listen(root, false);
    listen(join(root, ".git"), false);
  }
  return () => {
    if (timer !== null) clearTimeout(timer);
    for (const watcher of watchers) watcher.close();
    watchers.length = 0;
  };
}

/**
 * Whether a burst says anything the page should show. What git writes inside its own directory
 * matters only when it is the index, `HEAD`, a ref or a stash; a write under a directory git ignores
 * (`ignored`, each ending in `/`: a build's `target/`, `node_modules/`) never does.
 */
export function relevant(paths: readonly string[], ignored: readonly string[] = []): boolean {
  if (paths.length === 0) return true;
  const shown = (path: string) => {
    if (path.startsWith(".git/")) return path === ".git/HEAD" || path === ".git/index" || path.startsWith(".git/refs/") || path.startsWith(".git/logs/");
    return !ignored.some((dir) => path.startsWith(dir) || `${path}/` === dir);
  };
  return paths.some((path) => path === "" || shown(path));
}
