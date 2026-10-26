// Which of the plugins' `panel` components each of a theme's docks holds. A dock's `panels` are
// globs over a component's key (`owner/name/component`, `*` for any run of characters): a component
// goes to the first dock, in the order the theme declares them, with a pattern that names it; `*`
// alone names nothing, it takes whatever no dock names. Nothing here draws or keeps state.

/** Whether `key` fits `pattern`: `*` is any run of characters, anything else itself. */
export function glob(pattern: string, key: string): boolean {
  const parts = pattern.split("*").map((part) => part.replace(/[.+?^${}()|[\]\\/]/g, "\\$&"))
  return new RegExp(`^${parts.join(".*")}$`).test(key)
}

/** The keys each dock holds, by its name, of `keys` (in their order): every dock is in the map. */
export function assign(docks: [name: string, panels: readonly string[] | undefined][], keys: readonly string[]): Map<string, string[]> {
  const held = new Map<string, string[]>(docks.map(([name]) => [name, []]))
  const rest = docks.find(([, panels]) => panels?.includes("*"))?.[0]
  for (const key of keys) {
    const named = docks.find(([, panels]) => panels?.some((p) => p !== "*" && glob(p, key)))?.[0] ?? rest
    if (named) held.get(named)!.push(key)
  }
  return held
}
