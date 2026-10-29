# Contributing to pumi

Read [Plugins](docs/user/plugins.md) before changing code: the API the app, the CLI and the
plugins share is the one it describes.

## Build and run

You need Rust (pinned in `rust-toolchain.toml`), Zig 0.16.0 (on `PATH`, or `$ZIG`) and Bun. On
macOS also the Xcode command line tools, on Debian or Ubuntu what `sudo -E
scripts/release/linux-deps.sh` installs, and on Windows the MSVC build tools.

```sh
git clone https://github.com/pumi-dev/pumi && cd pumi
(cd runtime/app && bun install)                   # the app's JS dependencies
(cd runtime/app && bun run addon)                 # the addon (the root crate), debug
cargo build -p pumid -p pumi-host -p pumi-cli     # the server, its host and the CLI
bun runtime/app/app/main.ts                       # the debug app
```

The debug app runs as `pumi-dev`, with its own server and its own files (`~/.config/pumi-dev`,
…), so it never touches an installed pumi. For a release build, `scripts/release/build.sh` builds
one for this machine, and on macOS `./scripts/install.sh` builds `pumi.app` and puts it in
`/Applications`. `./scripts/install.sh --dev` installs `pumi Dev.app` beside it instead: a release
build with a blue icon that runs as `pumi-dev`, like the debug app, so you can work on pumi from
pumi while trying your changes in pumi Dev.

## Test

What CI runs on every pull request:

```sh
cargo build --locked -p pumid -p pumi-host -p pumi-cli
(cd runtime/app && bun run addon)
cargo xtask api --check                           # the generated API is current
cargo xtask platform                              # OS code is where it belongs
cargo check --locked -p pumi-gpuix                # the renderer builds without JS (no `napi`)
(cd runtime/app && bunx tsc --noEmit -p .)
CARGO_INCREMENTAL=0 cargo test --workspace --exclude xtask
(cd runtime/app && bun test)
```

CI runs all of it on macOS, everything but `cargo xtask api --check`, `tsc` and `bun test` on
Linux, and the server crates' tests (`-p pumid -p pumi-host -p pumi-cli -p pumi-sys`) on Windows.
`bun test` needs the debug addon: a release one has no test renderer.

## The API

A function of the API is a Rust function with `#[api]` (`crates/pumi-api-macros`), in `pumid`,
`pumi-host`, `pumi-os` or `pumi-api`'s builtins. After adding or changing one, run `cargo xtask api`: it checks each
function's examples, writes `api/pumi-api.json` and `api/app-functions.json`, and regenerates the
SDK's `runtime/app/worker/sdk/*.gen.ts` from them. It also writes the UI protocol
(`crates/pumi-ui-proto`: the ops a UI sends the renderer, the events it gets back, the style) to
`api/ui-protocol.json`, and `runtime/app/native/protocol.gen.ts` from it. Commit what it writes; CI
fails when it is stale. `api/plugin-manifest.schema.json` is not generated: keep it in step with
`crates/pumi-api/src/manifest.rs`.

## Style

- Rust is formatted with `cargo fmt`, as `rustfmt.toml` says (120 columns).
- Code for one OS goes in a file named for it (`macos.rs`, `linux.rs`, `windows.rs`, `unix.rs`) or
  in `crates/pumi-sys`; everywhere else builds on every OS, with no `#[cfg]` that names one.
  `cargo xtask platform` checks it.
- TypeScript passes `tsc` as `runtime/app/tsconfig.json` sets it (strict). A plugin's template
  checks itself the same way (`bun run check`). There is no formatter for it: follow the file
  you are in.

## Where things live

| Path | |
| --- | --- |
| `crates/pumid` | The server: sessions, views, panes, tabs, options, events and terminals. It routes every other call to the channel that provides it. The core's options are in `src/kernel/options.rs`. |
| `crates/pumi-host` | What runs beside the core: plugins (installing them, their files), keys, menus, settings, and the tab methods the core leaves to it (`wait`, `report`). |
| `crates/pumi-cli` | The `pumi` command. |
| `crates/pumi-api`, `crates/pumi-api-macros` | The API's registry and dispatcher, JSON-RPC 2.0, the plugin manifest, and `#[api]`. |
| `crates/pumi-proto` | What the server, the app and the CLI share: paths, the native protocol, the API's vocabulary. |
| `crates/pumi-model` | The model every core and client shares, and the pure queries over it. |
| `crates/pumi-layout` | A view's split tree and its geometry. |
| `crates/pumi-sys` | The OS layer: sockets, terminals, processes, signals, directories; a file per OS. |
| `crates/pumi-mirror` | The core's model as the host sees it, and the declarations of the options outside the core (`src/options.rs`). |
| `crates/pumi-keys`, `crates/pumi-lang` | Key names and their bytes for terminals; key chords and command-line quoting for the host and the app. |
| `crates/pumi-os` | The functions only the OS answers: secrets, the clipboard, `fs.*`, `process.*`, notifications. |
| `crates/pumi-ui-proto` | The UI protocol: the ops a UI sends the renderer, the styles they set and the events it sends back, as plain data with a JSON Schema. No JS, no GPUI. |
| `crates/pumi-gpuix` | The renderer: a tree a UI builds with the protocol's ops, drawn by GPUI. Plain Rust (`Renderer`); its `napi` feature adds the addon's JS classes (`src/addon`). Forked from gpuix. |
| `crates/pumi-ui` | GPUI components for the app's native windows (Settings). |
| `src/` | The root crate: the app's native half, a napi addon (the renderer, the connection to `pumid`, the terminals, the updater). |
| `runtime/app/app` | The app's TypeScript: the engine, commands and keymap, the default theme (`theme/`), Settings (`settings/`). |
| `runtime/app/sdk` | `pumi` as a plugin's `ui.tsx` imports it. |
| `runtime/app/worker` | Plugin workers, and `pumi` as a plugin's `main.ts` imports it (`sdk/`). |
| `runtime/app/react`, `runtime/app/native` | React's bindings to the renderer. |
| `runtime/app/gen` | What writes the SDK's `*.gen.ts` from `api/pumi-api.json`. |
| `runtime/app/template` | What `pumi plugin new` writes. |
| `xtask` | `cargo xtask api`, `platform` and `budget` (the core's size). |
| `scripts/` | Building from a checkout, and the release scripts (`scripts/release/`, see [Releasing](docs/releasing.md)). |
| `vendor/` | GPUI, from Zed. |

## License of contributions

A contribution is accepted under the license of the part it touches. The crates NOTICE lists as
Apache-2.0 (`pumid`, `pumi-proto`, `pumi-api`, `pumi-api-macros`, `pumi-sys`, `pumi-layout`,
`pumi-model`, `pumi-cli`, `pumi-gpuix` and `pumi-ui-proto`) take contributions under the Apache License 2.0
([LICENSE-APACHE](LICENSE-APACHE)); everything else under the GPL, version 3 or later
([LICENSE](LICENSE)). Each crate's `Cargo.toml` names its license. An Apache-2.0 crate includes no
GPL code; keep it that way. See [NOTICE](NOTICE) for the third-party code in the tree.
