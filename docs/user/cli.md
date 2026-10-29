---
title: Command line
group: Get started
order: 2
description: Tabs, their status, settings, keys and every API function, from a shell.
---

`pumi` is on the `PATH` of every tab. It talks to the server, so it works with the window closed, and every function is a command: its name in words, its id first.

```sh
pumi tab open pumi_dev.terminal.shell --split right -- pi --model opus "fix the tests"
pumi tab list --block pumi_dev.terminal.shell
pumi tab read %3 --lines 80
pumi open README.md
pumi event on 'tab.*'
```

{{usage}}

## Commands

There are no tmux verbs. `pumi NOUN VERB [ID] [ARGS…] [--flags]` is a function: `pumi tab get %3` is `tab.get("%3")`, and `pumi tab read` is `tab.call`'s `read` method. The flags are the options of the function's last argument, in kebab case: `--title x` is `{title: "x"}`, `--no-focus` is `false`, and `--state` is `true`.

An argument is JSON when it reads as JSON (`3`, `true`, `{"a":1}`) and text when it does not. A function whose one argument is an object takes its required fields first, as words: `pumi tab cells %3 --lines 5` is `tab.cells({tab: "%3", lines: 5})`. After `--` come a program and its words, for a command that runs one: `pumi tab open pumi_dev.terminal.shell -- htop` is `{command: ["htop"]}`, and `pumi tab run %3 -- npm test`. `pumi_dev.terminal.shell` is the [Terminal plugin](https://github.com/pumi-dev/plugins/tree/main/terminal)'s block, a shell in a tab. `get` prints the model; `tab read` prints the text; `list` prints a table on a terminal and JSON when it is piped. `pumi tab list` is a table either way, and `--json` says JSON (`tab list --json` is the ids).

A tab is an id, `%3` (`3` will do, where a tab is what is asked for); a pane `:4`, a view `@2`, a session `$1`, a request `+4`. `$PUMI_TAB`, `$PUMI_VIEW` and `$PUMI_SESSION` are yours, and a command that leaves its session or view out means them; one that leaves its pane out means your tab's.

Where a tab opens is its place, and its kind follows from its flags: `--split`, `--pane`, `--beside`, `--size` are a tiled place's, `--anchor`, `--width`, `--row` a floating one's; otherwise say `--kind tiled|float|view|session`. `--split right` opens it in a pane of its own beside yours; `--pane :4` opens it as a tab of pane `:4` (`--pane current` is yours), after the tab in front, and brings it there unless `--no-focus`. `pumi tab update %3 --place '{"kind": "tiled", "pane": ":4", "index": 0}'` moves a tab to the start of a pane's strip. `pumi pane focus --direction left`, `pumi pane update --zoom toggle` and `pumi pane close` act on a pane; `pumi tab focus --offset 1` goes to the next tab in yours.

| | |
| --- | --- |
| `pumi NOUN VERB [ID] [ARGS…] [--flags]` | every function: `pumi registry list fn` lists them |
| `pumi call NAME [args…]` | a function by name, when spelling it is easier |
| `pumi tab read\|input\|run\|wait\|selection\|report\|attach [TAB] [--flags]` | what a tab's block answers (`tab.call`) |
| `pumi tab list` | what runs where: tab, view, pane, block, status, title, directory |
| `pumi open PATH [--line N]` | a file in a tab: `tab open file '{"path": "PATH"}'` |
| `pumi wait [TAB]` | until the tab's program ends, with its status: `tab wait --exit` |
| `pumi event on EVENT` | events as they fire; `--match`, `--once` |
| `pumi do FILE` | run a TypeScript file with the SDK |
| `pumi help [NAME]` | every command, one a line; or a function, a tab method (`tab.read`), an event, a block, or a noun's commands |
| `pumi registry list KIND --names` | each function's (block's, option's, …) name and what it is, one a line |
| `pumi completion zsh\|bash\|fish` | what your shell completes for pumi |
| `pumi plugin new\|dev\|logs` | [Plugins](plugins.md): make one, run it as you edit it, read its log |
| `pumi server list\|stop` | the server processes themselves |

Installing, updating and removing a plugin are functions: `pumi plugin add SOURCE`, `pumi plugin update PLUGIN --enabled`, `pumi plugin remove PLUGIN`, and `pumi registry list plugin` shows what there is.

An error is one object on stderr, `{"kind": "not_found", "message": "no tab %7"}`, and `pumi` exits 1. A command line that names nothing to do exits 2. `pumi wait` exits with its program's status, and `pumi do` with its script's.

Global flags: `--json`, `--server NAME` (or a socket's path), `--socket PATH`, `--headless`, `--clean`, `--version`, `--help`.

## Status

A tab's `@status` says how what runs in it is doing: `working`, `blocked`, `error`, `done`, `idle` or `unknown`. The side bar shows a row for each tab that has one. A terminal's program reports it with an escape sequence (OSC 7501), and until one does pumi reads what it says otherwise: OSC 9;4 progress is `working` (or `error`), a braille spinner at the start of its title (OSC 0/2) is `working` until it stops, and a desktop notification (OSC 9, 99, 777) is `done` until you look at the tab. A plugin's block may set its own tabs' `@status`.

pumi has no agents of its own: they are the [Agents plugin](https://github.com/pumi-dev/plugins/tree/main/agents). It opens Claude Code, Codex and pi in terminals with their hooks on, which set the tab's status, and can show each as a chat of the same session, which queues prompts while the agent works.

```sh
pumi plugin add pumi-dev/plugins/agents     # the Agents plugin
pumi call pumi_dev.agents.open claude       # Claude Code in a terminal, its hook on
pumi tab list                               # every tab: its block, status, title and directory
pumi tab read %3                            # its screen's last 50 lines, as text
printf '\e]7501;state=working\e\\'          # working | blocked | done | idle | error
```

## When the app is slow

Window ▸ Diagnostics (`diagnostics` in the palette) opens a window that measures the app once a second while it is open: each window's frames (p50, p95 and max) and the main thread's slowest work (with the source it came from), every terminal's cost and how long it took to show, the app's JS (its event loop, React's re-renders, server calls), and the CPU, GPU and memory of pumi, pumid, pumi-host and, on macOS, WindowServer. Closed, nothing is measured.

## Settings

Every setting is an option. `pumi option set` changes it, and the server keeps it with the rest of its state; `settings.json` is where you write one yourself, and edits to it apply within a second. Options can be set globally, on one session, view or tab, or for one plugin (`--scope`, `--id`, `--plugin`).

```sh
pumi option set linespace 10
pumi option get linespace
pumi registry list option --name linespace
```

```json
// ~/.config/pumi/settings.json (Windows: %APPDATA%\pumi\settings.json)
{ "options": { "ui_theme": "default", "haptics": false } }
```

The terminals' settings are kept in `~/.config/pumi/config.ghostty` instead, in [Ghostty's config syntax](https://ghostty.org/docs/config), so Ghostty's lines work as they are. Settings → Appearance writes it, imports your Ghostty config into it (Import from Ghostty), and opens it in your editor (Edit file); edits apply within a second. The keys pumi reads: `font-family` (repeated for fallbacks), `font-size`, `font-style`, `font-thicken`, `font-feature` (`-calt`/`-liga` turn ligatures off), `adjust-cell-height`, `theme`, `background`, `foreground`, `cursor-color`, `selection-background`, `palette`, `palette-generate`, `palette-harmonious`, `minimum-contrast`, `background-opacity`, `background-blur`, `cursor-style`, `cursor-style-blink`, `window-padding-x`, `window-padding-y`, `window-padding-color` (`extend`, the default, carries a full-screen program's background into the padding; `background` leaves it the theme's), `copy-on-select` and `macos-option-as-alt`. Other keys stay in the file, unused.

```ini
# ~/.config/pumi/config.ghostty
font-family = JetBrains Mono
font-family = Symbols Nerd Font Mono
font-size = 13
adjust-cell-height = 10%
cursor-style = bar
theme = Dracula
```

{{options}}

{{other-options}}

## Keys

A key goes first to the app's keymap, which runs window commands like `split_vertical`. If the keymap doesn't take it, the server's `keys` option binds it to any API call, after the prefix or without one. Anything left goes to the tab.

```json
{ "options": { "keymap_bindings": { "split_vertical": ["cmd+\\"], "zoom": [] } } }
```

```sh
pumi option set keys '{"prefix":{"h":{"call":"tab.focus","args":[null,{"direction":"left"}]}}}'
```

{{keymap-bases}}

{{actions}}

Prefix `C-b`, then:

{{prefix-keys}}

## API

The server speaks JSON-RPC 2.0 on the socket in `$PUMI` (a Unix socket; on Windows a named pipe), one message per line. These are the functions the SDK, `pumi`, keys and menus reach. `pumi registry list fn --json` prints all of them.

```sh
printf '%s\n' '{"jsonrpc":"2.0","id":1,"method":"tab.list","params":[{}]}' | nc -U "$PUMI"
```

An error has a `kind`, and `pumi` exits 1 with it.

{{api-errors}}

{{api-functions}}
