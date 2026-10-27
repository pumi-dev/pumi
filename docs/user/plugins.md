---
title: Plugins
group: Build
order: 4
description: Add functions, tabs, buttons and components. TypeScript on Bun, React in the window.
---

A plugin is a folder with a `package.json` that has a `"pumi"` key. The manifest says what it adds, and pumi reads it without running any code. `src/main.ts` is its logic, in a Bun worker with the whole API. `src/ui.tsx` is what it draws, in React.

```text
hello/
  package.json     the manifest
  src/main.ts      functions, events, requests
  src/ui.tsx       components and renderers
  themes/<name>/   optional, see Themes
```

## Quickstart

```sh
pumi plugin new hello     # write a starter plugin
pumi plugin dev hello     # link it and reload on save
pumi plugin logs hello    # its stderr, and why it last stopped
```

A plugin with an option, a command and a sidebar panel:

```json
{
  "name": "pumi-hello",
  "type": "module",
  "main": "src/main.ts",
  "pumi": {
    "name": "hello",
    "owner": "me",
    "api": 1,
    "contributes": {
      "options": { "greeting": { "type": "string", "desc": "What to say.", "default": "Hello" } },
      "commands": [{ "id": "hello", "title": "Hello: say it" }],
      "slots": { "panel": { "component": "Counter", "title": "Hello" } }
    }
  }
}
```

```ts
// src/main.ts
import { fn, request, self } from "pumi";

// The command `hello`: the palette, a key mapped to it, and `pumi call me.hello.hello`.
fn.set("hello", { desc: "Says hello." }, () => request.open({ kind: "notification", title: `hello from ${self.plugin.id}` }));

let n = 0;
export const bump = () => ++n;
```

```tsx
// src/ui.tsx
import { useState } from "react";
import { useOption, useTheme } from "pumi";
import { bump } from "./main.ts"; // an async call into the worker

export function Counter() {
  const t = useTheme();
  const [greeting] = useOption<string>("me.hello.greeting");
  const [n, setN] = useState(0);
  return (
    <div style={{ padding: 8, hover: { backgroundColor: t.hover } }} onClick={async () => setN(await bump())}>
      <text>{`${greeting}, ${n}`}</text>
    </div>
  );
}
```

Everything a plugin owns is prefixed with its id, `owner.name.`: options, functions, commands, events, blocks, traits. `api` is the API level it was written for; pumi won't run a plugin for a level it doesn't know.

## The manifest

`"pumi"` holds the plugin: `name`, `owner`, `api`, and `contributes`, what it adds.

| Key | |
| --- | --- |
| `options` | Its options, by key: `greeting` is `me.hello.greeting`, global unless it says otherwise, and what Settings lists. |
| `commands` | Palette entries, `{id, title}`. A command is a function `owner.name.<id>`; a key and a slot action run one. |
| `keys` | Default keys, `{key, command}`. |
| `slots` | What it adds to the window: a `component` (an export of `ui.tsx`) and `actions` (each runs a command). |
| `blocks` | Tabs it can hold: `{id, title, icon, version, strategy, methods}`. |
| `renderers` | Blocks it draws but did not define, by block, with the `ui.tsx` export that draws them. |
| `traits` | What a tab is beside its block: `{id, title, desc, fields}`. |
| `settings` | Its pages in Settings and its sections on one, each a `ui.tsx` export: see [Settings](#settings). |

Beside `contributes`, the `pumi` key takes `themes` (theme directories, see [Themes](themes.md)), `activation`, `uses`, `minPumi`, `isolate`, `icon` and `images`.

`activation` says when the worker starts: `onStartup`, `onBlock:<id>`, `onCommand:<id>` or `onEvent:<name>`; with none, when one of its blocks opens or one of its commands runs. `uses` (`network`, `processes`, `files`, `addons`) is what it needs, for whoever installs it.

## Functions

A function is the one thing everything else calls: `fn.set(name, spec, impl)` registers one under the plugin's prefix, and `call(name, args)` runs any function, its own or the API's.

```ts
import { call, fn } from "pumi";

fn.set("greet", { desc: "Greets.", effect: "read", paramsSchema: { type: "object", properties: { who: { type: "string" } } } }, (who: string) => `hello ${who}`);
await call("me.hello.greet", ["you"]);     // "hello you"
await call([{ name: "me.hello.greet", args: ["you"] }, { name: "tab.focus", args: ["%3"] }]);   // one batch
```

`null` withdraws a function: `fn.set("greet", null)`. A command, a key, a menu item and another plugin all call it by name.

## Slots

A slot is a place in the window. Give it a `component`, an export of `ui.tsx`, or `actions`, buttons that run a command. Components get the object they sit on as a prop.

```json
"slots": {
  "tab.toolbar": { "actions": [{ "id": "pin", "title": "Pin", "icon": "pin", "command": "pin" }] },
  "panel": { "component": "Clock", "title": "Clock" }
}
```

| Slot | Props |
| --- | --- |
| `window.overlay` `panel` `palette.actions` | |
| `session.header` `session.menu` | `session` |
| `view.actions` `view.badge` `view.menu` | `view` |
| `tab.toolbar` `tab.strip` `tab.overlay` `tab.menu` | `tab` |
| `agent.controls` `agent.footer` `agent.menu` | `tab` |
| `sidebar.footer` | `point` (the dock's name) |

A theme draws them with `useSlot(slot, args)`, from `pumi/theme`.

## Blocks

A block is what a tab holds: a terminal, a file, or one of a plugin's own. The `ui.tsx` export named for the block draws it, and its methods are how others drive it.

```json
"blocks": [{ "id": "page", "title": "Page", "methods": ["navigate"] }]
```

```tsx
export function Page({ tab, block, focused }: RendererProps) { … }
```

```ts
import { call, workspace } from "pumi";

const page = await workspace.openTab("me.hello.page", {}, { kind: "tiled", split: "right" });
await call("tab.call", page.id, "navigate", { url });
```

`tab.call(tab, method, args)` is what a tab's block answers: `read`, `input`, `run`, `wait`, `selection`, `report` and `attach`, and the block's own methods. The block's own live in the plugin's functions, so `navigate` above is `me.hello.page.navigate`: the tab's id first, then the args. A renderer says what its tab shows with `describe`, and reads what it was opened with by `args`.

`tab.open(block, args, place)` says where: `kind` is `tiled`, `float`, `view` or `session`, and leaving the place out puts the tab below the caller's.

```ts
await call("tab.open", "terminal", { command: ["make", "test"] }, { kind: "tiled", split: "right", size: "30%" });
await call("tab.open", "terminal", {}, { kind: "view" });
await call("tab.open", "terminal", {}, { kind: "float", width: 80, height: 24 });
```

A file is the core's `file` block, `{path, line, column}`: `call("tab.open", "file", { path })`, or `pumi open PATH`.

### Renderers

`renderers` draws a block that isn't yours. For files, `match` picks which files the renderer draws:

```json
"renderers": { "terminal": "Terminal", "file": { "export": "Editor", "match": ["*.md"] } }
```

The renderer whose glob fits a file best draws it. For one tab alone, set its `@tab_renderer` option to the plugin's id; the `renderers` option (`{"file": "me/hello"}`) picks per block. A file tab's own methods go to the plugin that draws it, the tab's `@viewer` option, which the app keeps equal to the renderer it chose.

## Models and `workspace`

Everything the server holds is a model: `Session`, `View`, `Pane`, `Tab`, `Request` and `Plugin`. A field is a getter over a live cache, a method is one call, and `watch(keys, fn)` hears the keys it names.

```ts
import { workspace } from "pumi";

await workspace.ready;                                  // the cache is filled: reads are sync
const view = workspace.currentSession?.activeView;
const tests = await workspace.openTab("terminal", { command: ["make", "test"] }, { kind: "tiled", split: "right" });
await tests.update({ title: "tests" });
const { exitCode } = await tests.wait({ exit: true, timeout: 60 });
tests.watch(["title", "cwd"], () => console.log(tests.title));
```

| `workspace` | |
| --- | --- |
| `currentSession` `currentView` `currentTab` | the caller's own |
| `sessions` | every session |
| `getSession(id)` `getView(id)` `getPane(id)` `getTab(id)` `getPlugin(id)` | one by its id (`getPlugin` is a call) |
| `tabs(filter)` `requests(filter)` `panes(view)` | what `tab.list`, `request.list` and `pane.list` find |
| `openTab(block, args, place)` `openRequest(request, opts)` | one call, answered with the model |

A model's fields read `undefined` until the cache holds it. What `openTab`, `tabs()`, `panes()` and `requests()` answer is filled, and so are the caller's own objects and every session after `workspace.ready`; a model reached through another's field (`session.views`, `pane.tabs`) or by its id may not be, so `await model.ready` first. It answers the model, and rejects when the object is gone.

```ts
const pane = await workspace.getPane(":4")!.ready;
const tabs = await Promise.all(pane.tabs.map((tab) => tab.ready));
```

What a tab is, in one read: `tab.block`, `tab.traits`, `tab.place`, `tab.title`, `tab.cwd`, `tab.foregroundCwd`, `tab.process`, `tab.scroll`, `tab.neighbors`, `tab.isFocused`, `tab.isZoomed`, `tab.options`. A view holds `layout` (its panes as they split, each with its `tabs` and the one in front), `activeTab`, `activePane` and `zoom`; a pane (`workspace.getPane(":4")`, `workspace.panes(view)`) holds its `view`, `tabs` and `activeTab`; a session holds its `views` and `activeView`. `pane.opened`, `pane.changed` and `pane.closed` say what changed of a view's panes.

## Tab methods

Every tab answers these; `tab.call(tab, method, args)` runs one, and the SDK has each as a method on `Tab`. `run` and `attach` are a terminal's alone, and a block has its own besides (see [Blocks](#blocks)).

| Method | |
| --- | --- |
| `read({source, lines, ansi})` | text: `visible`, `recent` or `unwrapped` |
| `input({text})`, `input({paste})`, `input({keys})` | typing, a paste, or keys like `ctrl+c`, `f5`, `M-x` |
| `run({command})` | a bracketed paste and Enter, as one write |
| `wait({match})`, `wait({regex})`, `wait({exit: true, timeout})` | until the tab shows it, or its program ends with `{exitCode}` |
| `selection()` | the selected text |
| `report({body})` | an OSC 7501 report, as the program would send it: whether it changed the tab's state |
| `attach({mode, takeover, cols, rows})` | a terminal's: a stream of its screen, `control` writes and `observe` only watches |

A terminal opens with `{command, cwd, env}` and, with `keep: true`, stays open after its program ends.

## Requests

A request is something shown to the user: a notification, a permission, or a question.

```ts
import { on, request, workspace } from "pumi";

await request.open({ kind: "notification", title: "Build finished", notification: { sound: "default", actions: [{ id: "logs", label: "Logs" }] } });
const ask = await workspace.openRequest({ kind: "permission", title: "Run `make deploy`?", tool: "Bash", options: [{ id: "allow", label: "Allow" }, { id: "deny", label: "Deny" }] });
on("request.closed", { match: ask.id }, (e) => console.log(e.data.answer));
```

`request.list({tab, kind})` returns ids, `request.get(id)` the request, and `request.close(id, {answer})` answers it. A notification has no options and is posted to the OS notification centre under its own group, with its actions, reply and sound. A permission or a question expires after five minutes, and needs something to answer it: with no listener for `request.opened`, opening one fails. There are at most 256 notifications open.

## Options

Every setting is an option. `option.get(name, opts)` reads one and `option.set(name, value, opts)` writes it; `null` forgets it, so it inherits again.

| Scope | |
| --- | --- |
| `global` | the whole app: `{scope: "global"}` |
| `session` `view` `tab` | one object: `{scope: "tab", id: "%3"}`, the caller's own without `id` |
| `plugin` | one plugin's own store, kept across a restart, `{scope: "plugin", plugin: "me/hello"}`: what `main.ts` gets when it names no scope |

```ts
import { option } from "pumi";

await option.set("linespace", 10, { scope: "global" });
await option.set("@build", "ok", { scope: "tab", id: "%3" });
await option.set("seen", true);                     // the plugin's own store
const greeting = await option.get("greeting", { scope: "global" });   // a declared one
```

A manifest's `options` are declared globally as `owner.name.<key>`, and Settings lists them. A `secret` one is kept in the OS's secret store (the Keychain, the Secret Service, the Credential Manager), never on disk. `ttl` makes a value expire, and with `source` and `seq` a report older than the last one from that source is dropped.

## Settings

Settings is a window of its own, drawn with the UI theme in use. A plugin with options and nothing else gets a page there made of them, grouped by each option's `group`. To draw more (a sign-in row, a QR code, a grid of positions), give it pages of its own, or sections on a built-in page (`general`, `appearance`, `keys`, `plugins`, after that page's rows) or on one of its pages:

```json
"settings": {
  "pages":    [{ "id": "remote", "title": "Remote", "icon": "globe", "component": "RemotePage", "keywords": ["phone", "qr"] }],
  "sections": [{ "id": "tailscale", "page": "general", "title": "Tailscale", "component": "TailscaleSection", "keywords": ["sign in"] }]
}
```

`component` is an export of `ui.tsx`, loaded when its page is shown. A page and a section are prefixed as options are (`tailscale` is `me.remote.tailscale`), and `registry.list("settings")` lists them. Search finds each by its `title` and `keywords`, without running it, and finds options by their label, description and choices. `openSettings(page, link)` opens Settings at a page and scrolls to what `link` names: an option (`me.remote.position`, its row is highlighted wherever it is drawn), a section, a page (`me.remote.remote`), or a built-in row (`server`, `updates`, `safe-mode`).

```tsx
import { Button, OptionRow, Row, Section, useOption } from "pumi";
import { signIn } from "./main.ts";
import { qrSvg } from "./qr.ts";

export function TailscaleSection() {
  const [account] = useOption<string>("me.remote.account");
  return (
    <Section id="me.remote.tailscale" title="Tailscale">
      <Row icon="link" title="Sign in to Tailscale" description="Access pumi from your other devices."
           control={<Button primary onClick={() => signIn()}>Sign In</Button>} />
      {account ? <img src={qrSvg(`https://pumi.dev/remote/${account}`)} style={{ width: 160, height: 160 }} /> : null}
      <OptionRow name="me.remote.position" />
    </Section>
  );
}
```

A QR code needs nothing from pumi: encode it with any JS library and show the SVG with `<img>`, or as a grid of `div`s (`pumi plugin new` writes one, `src/qr.ts`). Each page and section is drawn in an error boundary of its own: one that throws is a card with the plugin's name and "Show logs", and the rest of Settings goes on. In safe mode the built-in pages work and no plugin's page or section is shown.

The kit, from `"pumi"`, is what the built-in pages are drawn with, so a plugin's look native:

| Export | |
| --- | --- |
| `Page` `Section` `Row` | layout: a title, what it means, a leading `icon`, a trailing `control`, an `aside`; a `link` highlights it when named |
| `Switch` `Slider` `Stepper` `TextField` `Select` `Button` | controls; `Select` with `options` is Settings' own, without it the headless one |
| `KeyRecorder` `KeyChip` | records a chord (or up to three in a row), and shows one |
| `ChoiceGrid` | choices in rows × `columns`: a notification's position |
| `ImageCards` | choices as cards with a picture (`src`, or a `preview` element); `strip` for hundreds of them in one row |
| `OptionRow name` | the row Settings makes for a declared option: its label, description, the control its type calls for, and a reset |
| `useSettings` `haptic` `openSettings` `keyGlyphs` | what a component is drawn in; a trackpad tap; open Settings; `cmd+k` as `⌘ K` |

`Switch` and `Slider` tap a Force Touch trackpad as they turn and step (the theme's `haptics` setting turns it off: Settings → Appearance under the default theme); call `haptic("alignment" | "levelChange" | "generic")` for a control of your own.

## Registries

One call describes everything that is registered: `registry.list(kind, opts)`, with `{name}` for one entry.

| Kind | |
| --- | --- |
| `fn` | every function, core and plugin, with its spec; `{palette: true}` the palette's |
| `block` `trait` `option` `key` `menu` `theme` `tabMethod` `model` | what is registered of that kind (`pumi help EVENT` describes an event) |
| `settings` | the plugins' pages and sections in Settings (`kind`, `plugin`, `id`, `title`, `component`, `keywords`, a section's `page`) |
| `plugin` | installed plugins; `{available: true}` searches the store as well |
| `viewer` | with `{path}`, the plugins that would draw that file |

`menu.set(path, item)` puts one in a menu and `null` takes it out.

## Events

Each session, view, pane and tab has `opened`, `closed` and `changed {keys, patch}`; a model's `watch` is a filter over `changed`. What else fires: `tab.alerted {kind, message}`, `request.opened` and `request.closed {answer, reason}`, `agent.sending` and `agent.turned`, `option.changed`, `registry.changed {kind, names}`, and `app.changed {keys, patch}` (the window's `isFocused`).

`on(event, opts, fn)` runs `fn` with each one and returns its disposer; `opts.match` is a glob or a list of globs tested against the event's `match` (an object's id, a tab's block, a request's kind). `opts.once` hears one.

```ts
import { emit, on } from "pumi";

const off = on("tab.changed", { match: "%3" }, (e) => console.log(e.data.keys));
on("tab.alerted", (e) => emit("bell", { message: e.data.message }));   // me.hello.bell
off();
```

`event.wait(event, opts)` is the next one, or `null` after its timeout. A plugin's own events are `owner.name.<name>`; `emit` prefixes them for you. `e.session`, `e.view` and `e.tab` say which objects the event is about.

## Agents

A tab is an agent while its `@agent` option is set; `@agent.name`, `@agent.status`, `@agent.session`, `@agent.model`, `@agent.transcript`, `@agent.conversation` and `@agent.accepts` describe it, and `tab.traits.agent` has them with its `queue`.

A plugin can tell agents things without typing into them: hidden context when they are sent a message, or a message queued until their turn ends.

```ts
import { agent, call } from "pumi";

agent.onSend((turn) => (turn.event === "prompt" ? "Context for the agent" : null));
agent.onTurn((turn) => console.log(turn.event));       // stop, compact
await call("agent.queue", tab.id, "run the tests again");
```

An agent transport builds a tab up from `agent.turn` and takes its queue with `agent.take(tab, {wait, session})`, then `agent.unqueue(id)`.

## Worker and components

`ui.tsx` calls `main.ts`'s exports as async functions. The other way, `ui.post` streams messages to the plugin's own components, fast enough for tokens. Components hear the server's events with `useEvent`, and re-render from the models.

```ts
ui.post("delta", { text });                                                // main.ts
const text = useChannel("delta", "", (all, d) => all + d.text);            // ui.tsx
useEvent("option.changed", (e) => {}, { match: "@agent.status" });         // ui.tsx
```

## Web pages

`<WebView>` shows a plugin's HTML in a component. The page gets `window.pumi` for the API and the theme.

```tsx
<WebView handle={`chart-${tab.id}`} src="web/chart.html" args={{ tab: tab.id }} />
```

```js
// web/chart.html
pumi.call("tab.list").then(render);
pumi.on("tab.changed", { match: pumi.tab }, render);
pumi.onTheme((t) => (document.body.style.color = t.text));
```

## Files

`pumi plugin dev` links a folder, so its own files are what runs. What it keeps between runs goes in `paths`: `paths.config`, `paths.state` and `paths.cache`, a directory each under pumi's, for that plugin alone. An option in the `plugin` scope is the store pumi keeps for it.

## Share

```sh
pumi plugin add owner/repo                   # install from GitHub
pumi plugin add owner/repo --version v1.2   # a version of it
pumi plugin update owner/name                # refresh it from its source (its id, which may differ from the repo's name)
pumi plugin remove owner/name                # uninstall it
```

A plugin runs with your permissions and no sandbox, so say what yours does.

A plugin from GitHub with `dependencies` in its `package.json` gets them on install, with `bun install --production --no-save --ignore-scripts` (its `devDependencies` and install scripts are skipped). `plugin update` installs them again when they change, and pumi installs any that are missing when it starts. A linked plugin's are yours to install.

## SDK

Both files import from `"pumi"`.

| `main.ts` | |
| --- | --- |
| objects | `workspace` `call` `fn` `option` `registry` `request` `menu` `event` `plugin` `fs` `clipboard` `process` |
| own | `on` `emit` `self` `paths` `layout` `stop` `ui.post` |
| agent | `agent` (`onSend`, `onTurn`, `turn`, `queue`, `take`, `unqueue`) |
| models | `Session` `View` `Pane` `Tab` `Request` `Plugin`, and their data types |

| `ui.tsx` | |
| --- | --- |
| state | `useSessions` `useSession` `useViews` `useView` `useTab` `useTabs` `useRequests` `useRegistry` `useWatchable` `useOption` `useWindow` `workspace` `call` |
| models | `Session` `View` `Pane` `Tab` `Request` `Plugin` `Model` `cache` |
| theme | `defineTheme` `useTheme` `usePalette` `useMetrics` `useAppearance` `useFont` |
| hooks | `useChannel` `useChannelListener` `useEvent` `usePlugin` |
| elements | `div` `text` `img` `svg` `scroll` `input` `terminal` `Icon` `IconButton` |
| web | `WebView` `useWebview` `webview` |
| kit | `motion` `Select` `useDraggable` `useDroppable` `useDragState` `useHover` `useInterval` `useNow` |
| settings | `Page` `Section` `Row` `OptionRow` `Switch` `Slider` `Stepper` `TextField` `Button` `KeyRecorder` `KeyChip` `ChoiceGrid` `ImageCards` `useSettings` `haptic` `openSettings` `keyGlyphs` |
