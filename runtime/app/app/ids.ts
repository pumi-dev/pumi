// The app's wire ids (`%3`, `:4`, `@2`, `$1`, `devbox/%3`): which server an id is on, and its number
// there. Apart from the engine, so code that only reasons about ids does not load the addon.

/** The number the API takes for a wire id (`%3`, `:4`, `@2`, `$1`), on whichever server it is
 * (`devbox/%3` is `3` on `devbox`: `via` names the server). */
export function num(id: string): number {
  return Number(id.slice(id.indexOf("/") + 1).replace(/^[%:@$]/, ""))
}

/** The remote server an id is on (`devbox` for `devbox/@2`); none for this one's. */
export function serverOf(id: string | null | undefined): string | undefined {
  const at = id ? id.indexOf("/") : -1
  return at > 0 ? id!.slice(0, at) : undefined
}

/** `method` on the server `id` is on: `devbox/view.close` for `devbox/@2`, or `method`. */
export function via(id: string | null | undefined, method: string): string {
  const server = serverOf(id)
  return server ? `${server}/${method}` : method
}

/** A wire id a server answered (`$3`), as this app names it (`devbox/$3`). */
export function idOn(server: string | undefined, id: string): string {
  return server ? `${server}/${id}` : id
}
