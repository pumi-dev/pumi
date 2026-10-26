// The window a command runs in, for whatever draws the window: the app registers how to find one
// (`setContextResolver`), and a theme — the app's own, or a plugin's — only ever holds a window's
// id (`useWindow().id`). A theme that redraws the whole window needs this to run a command from a
// click; a part that the default window draws gets its context from there.
import { run as runIn, type Context } from "../app/commands.ts"

let resolve: (id: number) => Context | undefined = () => undefined

/** The app's own: how to find the context of the window `id`. */
export function setContextResolver(get: (id: number) => Context | undefined) {
  resolve = get
}

/** The context of the window `id`: what a command run there acts on. */
export function windowContext(id: number): Context | undefined {
  return resolve(id)
}

/** Runs the command `name` in the window `id` (`useWindow().id`). */
export function run(name: string, id: number): void {
  const context = resolve(id)
  if (context) runIn(name, context)
}
