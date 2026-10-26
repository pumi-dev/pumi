# A pumi plugin

`package.json`'s `"pumi"` key is the manifest: what the plugin contributes, read without running
it. `src/main.ts` is its logic — a worker with the whole API (`fn`, `on`, `request`, `option`,
`workspace`, …) — and `src/ui.tsx` what it draws, in React, in the window: the hooks of `pumi`,
colours from `useTheme()`, style objects. `ui.tsx` imports `main.ts`'s exports and calls them as
async functions.

- `contributes.options`: its options, by key: `size` is `owner.name.size`, global unless the
  manifest narrows the scope, and listed in Settings. Read one from `main.ts` with
  `option.get("size", { scope: "global" })` and from a component with `useOption("owner.name.size")`.
  What the plugin keeps that no manifest declares is its own store: `option.get("seen")`.
- `contributes.commands`: `[{id, title}]`, the palette's entries. A command is the function
  `owner.name.<id>`, which `main.ts` registers with `fn.set("<id>", { desc }, impl)`; a key, a
  menu item and a slot action run it by name.
- `contributes.keys`: default keys, `[{key, command}]`.
- `contributes.slots.<slot>`: `actions` (each runs a command) and a `component` (an export of
  `ui.tsx`). Slots: `panel`, `sidebar.footer` (a button at the side bar's foot, running a
  command), `window.overlay`, `session.header`, `view.actions`, `view.badge`, `tab.strip`,
  `tab.overlay`, `tab.toolbar`, `agent.controls`, `agent.footer`, the menus `session.menu`,
  `view.menu`, `tab.menu` and `agent.menu`, and `palette.actions` (actions only).
- `contributes.blocks`: what a tab can hold, drawn by the `ui.tsx` export named for the id
  (`board` → `Board`, given `{ tab, block, focused }`). Its own methods are the plugin's
  functions `<block>.<method>`, so `tab.call("%3", "back")` on a `owner.name.browser` tab calls
  `owner.name.browser.back` with the tab's id and then the args. `contributes.renderers` draws a
  block it did not define (`{ "terminal": "MyTerminal" }`); the core's `file` block (a file in a
  tab, `tab.open("file", {path})`) is drawn by the renderer whose globs fit the file best
  (`{ "file": { "export": "Viewer", "match": ["*.md"] } }`).
- `contributes.traits`: what a tab is beside its block. A tab has trait `x` while its option
  `@owner.name.x` is set; its fields are `@owner.name.x.<field>`.
- Everything of its own is prefixed `owner.name.`.
- A notification, a permission or a question is a request: `request.open({ kind, title })`.
- `main.ts` → its components, live: `ui.post(topic, data)`, heard by `useChannel(topic, initial)`;
  server events in a component: `useEvent(name, fn, { match })`.
- What it keeps between runs: `paths.config`, `paths.state`, `paths.cache`.

```sh
pumi plugin add .     # link it and load it
pumi plugin dev .     # reload on save
pumi plugin logs {{name}}
```

Reference: https://pumi.dev/docs/ and `pumi help`.
