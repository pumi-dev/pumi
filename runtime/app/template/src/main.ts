// The plugin's logic: it runs in a worker of the app's Bun, on the app that hosts plugins. `pumi`
// is the API (the SDK the app ships); it adds the plugin's prefix (`owner.name.`) for you.
import { fn, request, self } from "pumi";

// The command `hello` (package.json's `contributes.commands`): the palette, a key mapped to it,
// the tab menu's item and `pumi call <owner>.<name>.hello` run it.
fn.set("hello", { desc: "Says hello." }, () => request.open({ kind: "notification", title: `hello from ${self.plugin.id}` }));

let clicks = 0;

/** An export: `ui.tsx` imports it and calls it as an async function, answered here. */
export function click(): number {
  return ++clicks;
}
