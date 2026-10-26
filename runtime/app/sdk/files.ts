// Which files a renderer of the core's `file` block draws: the globs of its manifest's
// `renderers.file.match`, as pumi-api's `glob_fit` reads them (crates/pumi-api/src/manifest.rs).

/**
 * How well `glob` matches `path`: its count of literal characters, or `undefined`. A glob without
 * a `/` is matched against the file's name, one with a `/` against its path (from any folder,
 * unless it starts with `/`). `*` is any run of characters but `/`, `**` any at all, `?` one; case
 * is ignored. A Windows path is matched by its `/`, as a glob writes it.
 */
export function globFit(glob: string, path: string): number | undefined {
  path = path.replace(/\\/g, "/")
  const folders = glob.includes("/")
  const subject = (folders ? path : path.slice(path.lastIndexOf("/") + 1)).toLowerCase()
  const anchored = folders && !glob.startsWith("/") && !glob.startsWith("**") ? `**/${glob}` : glob
  const p = [...anchored.toLowerCase()]
  const s = [...subject]
  const walk = (i: number, j: number): boolean => {
    if (i === p.length) return j === s.length
    if (p[i] === "*" && p[i + 1] === "*") {
      const rest = p[i + 2] === "/" ? i + 3 : i + 2
      for (let k = j; k <= s.length; k++) if (walk(rest, k)) return true
      return walk(i + 2, j)
    }
    if (p[i] === "*") {
      for (let k = j; k <= s.length; k++) {
        if (k > j && s[k - 1] === "/") break
        if (walk(i + 1, k)) return true
      }
      return false
    }
    if (p[i] === "?") return j < s.length && s[j] !== "/" && walk(i + 1, j + 1)
    return s[j] === p[i] && walk(i + 1, j + 1)
  }
  return walk(0, 0) ? [...glob].filter((c) => c !== "*" && c !== "?").length : undefined
}

/** How well a renderer with these globs fits `path` (every file, at 0, without any). */
export function fit(match: string[] | undefined, path: string): number | undefined {
  let best: number | undefined
  for (const glob of match ?? ["*"]) {
    const n = globFit(glob, path)
    if (n !== undefined && (best === undefined || n > best)) best = n
  }
  return best
}
