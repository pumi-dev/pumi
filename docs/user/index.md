---
title: Install
group: Get started
order: 1
description: Install pumi on macOS, Linux or Windows, and find your way around.
---

pumi is a terminal app for running coding agents side by side. A window holds sessions, a session holds views, a view holds panes, and a pane holds tabs. Every part of it is a plugin you can turn off, replace or write yourself.

One line installs the app, its server and the `pumi` command, then opens the app. On macOS and Linux:

```sh
curl -fsSL https://pumi.dev/install.sh | sh
```

On Windows, in PowerShell:

```powershell
irm https://pumi.dev/install.ps1 | iex
```

pumi runs on macOS 14 or later (Apple silicon and Intel), Linux with glibc 2.35 or later (Debian 12, Ubuntu 22.04, Fedora 36, Arch; x86_64 and arm64), and Windows 10 1809 or later (x86_64).

Flags go after `sh -s --` (`curl -fsSL https://pumi.dev/install.sh | sh -s -- --version 0.42.0`):

| Flag | |
| --- | --- |
| `--version X` | Install a specific version. |
| `--dir PATH` | Where `pumi.app` goes. macOS: `/Applications`, or `~/Applications` when that is not writable. Linux: `~/.local`. |
| `--no-bin` | Don't put `pumi` on your `PATH`. By default the installer links it into `~/.local/bin` and, when that is not on your `PATH`, adds it in your shell's startup file (`~/.zshrc`, `~/.bash_profile` or `~/.bashrc`, fish's `conf.d`). |
| `--no-open` | Don't open pumi when it is installed. |
| `--uninstall` | Remove the app. Your settings stay. |

On Windows the same are `-Version`, `-Dir` (`%LOCALAPPDATA%\Programs\pumi` by default), `-NoOpen` and `-Uninstall`: `& ([scriptblock]::Create((irm https://pumi.dev/install.ps1))) -Version 0.42.0`. The installer adds `pumi` to your user `PATH` and pumi to the Start menu, and Settings → Apps can uninstall it.

The environment variables `PUMI_INSTALL_VERSION`, `PUMI_DIR`, `PUMI_NO_BIN` and `PUMI_NO_OPEN` do what the flags do. The installer checks the archive's SHA-256 against the release manifest, and the manifest's signature when `minisign` is installed. On Linux it also tells you which desktop libraries (GTK, WebKitGTK, xkbcommon, Vulkan) are missing.

Updates download in the background and apply on restart; set `autoupdate` to `false` to turn them off.

## From source

You need Rust (the version in `rust-toolchain.toml`), Zig 0.16 and Bun. On macOS you also need the Xcode command line tools. On Debian or Ubuntu, `sudo -E scripts/release/linux-deps.sh` installs what the build links. On Windows you need the MSVC build tools.

```sh
git clone https://github.com/pumi-dev/pumi
cd pumi
(cd runtime/app && bun install)   # the app's JS dependencies, first
./scripts/install.sh              # macOS: build pumi.app and put it in /Applications
scripts/release/build.sh          # any OS: a release build for this machine, into target/<triple>/release
```

## How it fits together

`pumid` is the server. It owns every session, view, pane and tab, and the programs in them, so they keep running when you close the window. The app draws the window and runs the plugins. The `pumi` command talks to the server, and is on the `PATH` of every tab.

```text
session ─ view ─ pane ─ tab      %3 is a tab, :4 a pane, @2 a view, $1 a session, +7 a request
```

A view is what the window shows: its panes, split side by side. A pane holds tabs, with a strip of them on top and one in front; a tab holds a terminal, an agent, a page or a file. Set `pane_tabs` to `false` for one tab to a pane, as in tmux.

## Files

Everything pumi keeps is in plain files you can read, edit and commit. On macOS and Linux:

| Path | |
| --- | --- |
| `~/.config/pumi/settings.json` | Options and keys you write yourself, `{"options": {…}, "keymaps": [{…}]}`: [Command line](cli.md) says what goes in it. Watched: edits apply within a second. |
| `~/.config/pumi/config.ghostty` | The terminals' settings (font, cursor, colours, padding), in Ghostty's config syntax: [Command line](cli.md#settings) lists the keys. Watched too. |
| `~/.config/pumi/plugin-lock.json` | Installed plugins and where they came from. |
| `~/.local/share/pumi/installed/` | The plugins themselves (their logs are beside them, in `plugins/`). |
| `~/Library/Application Support/pumi/` (Linux: `~/.local/share/pumi/`) | The server's state: its sessions, views, tabs and options, its log, and its socket (`$PUMI`). |
| `~/.local/state/pumi/plugins/<owner.name>/` | What a plugin keeps between runs. |

On Windows, settings and the plugin lock are in `%APPDATA%\pumi\`, the plugins in `%LOCALAPPDATA%\pumi\data\installed\`, what they keep in `%LOCALAPPDATA%\pumi\state\plugins\<owner.name>\`, and the server's state in `%LOCALAPPDATA%\pumi\`; `$PUMI` is a named pipe. A set `XDG_CONFIG_HOME`, `XDG_DATA_HOME` or `XDG_STATE_HOME` moves the matching directory on every OS.

## Next

- Drive pumi from a shell, set options and bind keys: [Command line](cli.md).
- Add a tab, a button or a command: [Plugins](plugins.md).
- Redraw the window: [Themes](themes.md).

## Work safely

Plugins run with your permissions: your files, your network, every terminal. pumi does not sandbox them. Install what you would run as code.

If a plugin or theme breaks the window, start the app with `--safe`: it draws the default theme and runs no plugin. Settings → Plugins → Safe mode does the same from inside the app.

```sh
open -a pumi --args --safe                                            # macOS
~/.local/pumi.app/bin/pumi --safe                                     # Linux
& "$env:LOCALAPPDATA\Programs\pumi\pumi.app\bin\pumi.exe" --safe       # Windows
```
