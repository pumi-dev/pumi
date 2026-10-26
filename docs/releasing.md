# Releasing pumi

How a version gets from a tag to installed copies, on macOS, Linux and Windows. The workflows
are in `.github/workflows/`, the scripts they call in `scripts/release/`.

## What ships

| Artifact | |
| --- | --- |
| `pumi-<version>-macos-<arch>.zip` | `pumi.app`, signed with the release certificate |
| `pumi-<version>-macos-<arch>-symbols.zip` | debug symbols, zipped apart |
| `pumi-<version>-linux-<arch>.tar.gz` | `pumi.app/`, unpacked into `~/.local` |
| `pumi-<version>-windows-x86_64.zip` | `pumi.app\`, unpacked into `%LOCALAPPDATA%\Programs\pumi` |
| `install.sh`, `install.ps1` | the two installers, as this tag has them |
| `latest.json` + `latest.json.minisig` | the channel's manifest and its signature |

Every archive also has a `.sha256` and a `.minisig`. `latest.json` names each platform's archive
(`macos-aarch64`, `macos-x86_64`, `linux-x86_64`, `linux-aarch64`, `windows-x86_64`) with its size
and SHA-256; its minisign signature is the root of trust, so an archive is trusted by matching the
signed manifest, wherever it was downloaded from. The `-symbols` zips and the `.sha256`/`.minisig`
files are not in it: nothing is installed from them.

All three layouts are the same bundle: an executable in `bin/` (`Contents/MacOS/` on macOS) with
the addon `pumi.<platform>-<arch>.node` beside it, and everything read from disk at run time under
`Resources/` (`runtime/`, `assets/`, `bin/pumi-cli`, the notices) — so the app finds its pieces the
same way on every OS.

## How each job builds it

| Job | Runner | What it produces |
| --- | --- | --- |
| `version` | `ubuntu-22.04` | nothing: it checks the tag names `Cargo.toml`'s version |
| `macos` | self-hosted Apple silicon | both Mac architectures, cross-compiled, `pumi.app` signed |
| `linux` (x86_64) | `ubuntu-22.04` | the tarball, built against glibc 2.35 |
| `linux` (aarch64) | `ubuntu-22.04-arm` | the tarball for arm64 |
| `windows` | `windows-2022` | the zip, built natively with Zig and Bun |
| `macos-intel-smoke` | `macos-15-intel` | nothing: the Intel zip run on Intel hardware, only while the variable `PUMI_INTEL_SMOKE` is `true` |
| `draft` | `ubuntu-22.04` | the manifest, the signatures, and the draft release |

Linux builds on 22.04 so an installed copy runs on any glibc 2.35 or later (Debian 12, Ubuntu
22.04, Fedora 36, Arch). Building there needs `scripts/release/linux-deps.sh` — the X11, Wayland,
GTK and WebKitGTK libraries gpui and wry link, Zig, and ImageMagick for the icons.

Every job runs `scripts/release/build.sh <triple>`, which builds the server crates for that target,
then the addon and the app (Bun compiles the app for another OS itself; `cargo xwin` links MSVC
binaries off Windows), and leaves each target's files in cargo's per-target directory. Then
`pack-macos.sh`, `pack-linux.sh` or `pack-windows.sh` assembles the bundle and packs it.

Windows is unsigned in v1. Nothing needs to be: `install.ps1` downloads with `Invoke-WebRequest`,
which sets no mark-of-the-web, and SmartScreen only ever looks at files that carry one — so the
executables it unpacks run without a prompt. What vouches for a download is the signed manifest:
the installer checks the archive's SHA-256 against `latest.json`, and the updater does the same.
When a code-signing certificate is worth its price, signing `pumi.exe` and the CLI changes nothing
about that chain — it only stops the "unknown publisher" line in the properties dialog.

## Cutting a release

1. Set `version` in the root `Cargo.toml`, build so `Cargo.lock` follows, and commit.
2. Tag and push: `git tag v0.42.0 && git push origin v0.42.0`.
3. `release` runs on the tag: it checks the tag names `Cargo.toml`'s version, builds every
   platform, packs and signs each bundle, smoke-tests them, then writes `latest.json` and signs
   everything. It leaves a **draft** release.
4. Try the draft: download an archive and run the app from it.
5. Publish the draft in GitHub's UI, or `gh release edit v<version> --draft=false`. Nothing is
   live before that: the updater and the installers only ever read the latest *published* release,
   which pumi.dev relays with its signature. Copies find it within four hours, at once with
   Settings → General → **Check for updates**.

Re-running the workflow for a tag (`gh workflow run release -f tag=v<version>`) replaces its draft.
It deletes whatever release has that tag first, published or not, so never re-run a published one.
A published release is never pulled back by downgrading — the updater only moves forward — so
ship a fixed version, and remember that deleting a release only stops new installs of it.

## Keys

| Secret | Used for |
| --- | --- |
| `MACOS_CERT_P12` + `MACOS_CERT_PASSWORD` (the variable `MACOS_CERT_SHA1` names it) | signing the Mac app |
| `MINISIGN_SECRET_KEY` | signing the release manifest and the archives, with the primary update key |

The certificate is not Apple's, and it does nothing for Gatekeeper: pumi opens without a prompt
because `install.sh` and the updater download with `curl`, which does not quarantine. What it does
is keep the app's designated requirement (`certificate root = H"<sha1>"`) the same across
releases, so macOS keeps file-access and keychain grants through updates, and the updater refuses
an update signed with another certificate. **Never change it** unless you mean every Mac to ask
for its permissions again. It signs nothing on Linux or Windows, where the bundles are unsigned.

The app trusts two update keys, the public keys in `src/updater/manifest.rs`: the primary, whose
secret key is `MINISIGN_SECRET_KEY` and signs every release, and a backup, for when the primary is
lost or leaked. `scripts/release/install.sh` and `scripts/release/install.ps1` duplicate them; the
three must agree. A manifest signed by either key is trusted. Keep the backup's secret key offline
(a password manager or an encrypted USB stick), never on a build machine or in a secret.

To replace a key, ship a release that names the new public key in its place in all three files,
signed with a key the installed copies already trust (the backup, when the primary is the one
lost); copies that update to it trust the new key and stop trusting the one it replaced. The
`draft` job checks the manifest's signature against the first key in `src/updater/manifest.rs`,
so the key that signs a release goes first there. If every key is lost, installed copies can no
longer verify updates and their users must reinstall once a build trusting new keys is out.

## Running the scripts by hand

`scripts/release/build.sh [triple…]` builds (this machine's target by default),
`pack-macos.sh <arch>`, `pack-linux.sh <arch>` and `pack-windows.sh` pack, sign and zip, and
`manifest.sh` writes and signs `latest.json` from what is in `$OUT`. `manifest.sh` without
`MINISIGN_KEY` is a dry run that signs nothing.

A Windows release can also be built on a Mac or a Linux machine — `build.sh x86_64-pc-windows-msvc`
— with `cargo install cargo-xwin` (napi fetches it for the addon): cargo links the binaries with
LLVM and Microsoft's libraries, and Bun compiles the app for Windows. That is for trying a build,
not for shipping: the Windows job builds every release natively.
