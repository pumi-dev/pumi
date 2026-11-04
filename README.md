<p align="center">
  <img src="assets/brand/app-icon.png" width="128" alt="pumi app icon">
</p>

<h1 align="center">pumi</h1>
Pumi is a minimal slopless, highly extensible workspace for agents that you can make your own. The core app is an empty shell, everything is a plugin.

<p align="center">
  <a href="https://pumi.dev/docs/">Docs</a> ·
  <a href="https://pumi.dev/plugins/">Plugins</a> ·
  <a href="https://discord.gg/p8RYkZU86">Discord</a>
</p>

<video src="https://github.com/user-attachments/assets/c1717980-296c-43fd-904a-eb885b93896a" controls muted>
  <img src="https://github.com/user-attachments/assets/2f7409f1-b802-4230-bae3-e3db29553de6" alt="Watch the demo"> 
</video>
  
## Install

macOS and Linux:

```sh
curl -fsSL https://pumi.dev/install.sh | sh
```

Windows, in PowerShell:

```powershell
irm https://pumi.dev/install.ps1 | iex
```

## Docs

- [Plugins](docs/user/plugins.md)
- [Themes](docs/user/themes.md)
- [Contributing](CONTRIBUTING.md)
- [Security](SECURITY.md)

## Build from source

Rust (pinned in `rust-toolchain.toml`), Zig 0.16.0 and Bun; plus the Xcode command line tools on
macOS, `scripts/release/linux-deps.sh` on Debian/Ubuntu, or the MSVC build tools on Windows.

```sh
git clone https://github.com/pumi-dev/pumi && cd pumi
(cd runtime/app && bun install)      # the app's JS dependencies, first
./scripts/install.sh                 # macOS: build and install pumi.app
scripts/release/build.sh             # any OS: a release build for this machine
```

Before changing code, read [Plugins](docs/user/plugins.md): the app, the CLI and plugins share that
API. [Contributing](CONTRIBUTING.md) has the dev build, the tests and the checks CI runs.

## License

The server and the crates to build on it are [Apache-2.0](LICENSE-APACHE). The app, its plugins and the rest are
[GPL-3.0-or-later](LICENSE). Third-party notices are in [NOTICE](NOTICE).
