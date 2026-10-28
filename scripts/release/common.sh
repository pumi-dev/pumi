# Shared by the release scripts: sourced, never run.
#
# The names every release artifact goes by, in one place, because `install.sh`, the app's
# updater (`src/updater`) and CI all have to agree on them:
#
#   pumi-<version>-macos-<arch>.zip       a `ditto` zip of pumi.app
#   pumi-<version>-linux-<arch>.tar.gz    a tarball of pumi.app/ (bin/, Resources/, share/)
#   pumi-<version>-windows-x86_64.zip     a `zip` of pumi.app\ (bin\, Resources\)
#   latest.json                           the channel's manifest (see manifest.sh)
#
# Each artifact has a `.sha256` and a `.minisig` beside it. <arch> is `aarch64` or `x86_64`.
# Everything here runs on macOS, Linux and (Git Bash on) Windows: the Windows job of a release
# packs its own artifact.

set -euo pipefail

REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
APP_NAME="pumi"
VERSION="${VERSION:-$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -1)}"
# CFBundleVersion: CI's run number, so every build sorts after the one before it.
BUILD_NUMBER="${BUILD_NUMBER:-1}"
CHANNEL="${CHANNEL:-preview}"
OUT="${OUT:-$REPO/dist/release}"
# Cargo's target directory: the checkout's, unless CARGO_TARGET_DIR moves it (CI keeps one
# outside the checkout, which `actions/checkout` cleans).
TARGET_DIR="${CARGO_TARGET_DIR:-$REPO/target}"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
die() { printf '\033[1m==>\033[0m \033[31m%s\033[0m\n' "$*" >&2; exit 1; }

# `aarch64-apple-darwin` -> `macos-aarch64`; the platform key in latest.json and the asset name.
platform_of() {
  case "$1" in
    aarch64-apple-darwin) echo macos-aarch64 ;;
    x86_64-apple-darwin) echo macos-x86_64 ;;
    aarch64-unknown-linux-gnu) echo linux-aarch64 ;;
    x86_64-unknown-linux-gnu) echo linux-x86_64 ;;
    x86_64-pc-windows-msvc) echo windows-x86_64 ;;
    *) die "no platform for target $1" ;;
  esac
}

# `x86_64-pc-windows-msvc` -> `win32-x64`: what Bun's `process.platform` and `process.arch` say
# for a target, which is what the app names its addon by.
node_target() {
  case "$1" in
    aarch64-apple-darwin) echo darwin-arm64 ;;
    x86_64-apple-darwin) echo darwin-x64 ;;
    aarch64-unknown-linux-gnu) echo linux-arm64 ;;
    x86_64-unknown-linux-gnu) echo linux-x64 ;;
    x86_64-pc-windows-msvc) echo win32-x64 ;;
    *) die "no node target for $1" ;;
  esac
}

# The addon's file in a release: beside the app executable, named as the app asks for it.
addon_name() {
  echo "$APP_NAME.$(node_target "$1").node"
}

host_target() {
  local arch os
  arch="$(uname -m)"
  [[ "$arch" == arm64 ]] && arch=aarch64
  os="$(uname -s)"
  case "$os" in
    Darwin) echo "$arch-apple-darwin" ;;
    Linux) echo "$arch-unknown-linux-gnu" ;;
    MINGW*|MSYS*|CYGWIN*) echo "$arch-pc-windows-msvc" ;;
    *) die "unsupported host $os" ;;
  esac
}

sha256_of() {
  if command -v shasum >/dev/null 2>&1; then shasum -a 256 "$1" | awk '{print $1}'; else sha256sum "$1" | awk '{print $1}'; fi
}

# `<sha>  <name>`, the form `shasum -a 256 -c` reads.
write_sha256() {
  printf '%s  %s\n' "$(sha256_of "$1")" "$(basename "$1")" > "$1.sha256"
}

file_size() {
  if stat -f%z "$1" >/dev/null 2>&1; then stat -f%z "$1"; else stat -c%s "$1"; fi
}

# The third-party notices every distribution carries, into <dir>.
copy_notices() {
  local dir="$1"
  cp "$REPO/assets/icons/LICENSES" "$dir/lucide-LICENSES"
  cp "$REPO/NOTICE" "$dir/NOTICE"
  cat > "$dir/NOTICES.md" <<'NOTICE'
# Third-party notices

* The app icon is pumi's own logo.
* The UI icons are Lucide icons (ISC) — see `lucide-LICENSES`.
* The terminal emulator is libghostty-vt (MIT), through its Rust binding libghostty-rs
  (MIT OR Apache-2.0) — see `NOTICE`.
NOTICE
}

# A tree copied into another, file by file: `find` and `cp` only, because a release is packed
# on Windows too, where there is no rsync. What a build leaves behind (node_modules, an addon,
# tests, Finder's droppings) stays out.
copy_tree() {
  local src="$1" dest="$2" file
  [ -d "$src" ] || return 0
  while IFS= read -r file; do
    case "$file" in
      */node_modules/*|*.node|*.test.ts|*/.DS_Store) continue ;;
    esac
    mkdir -p "$dest/$(dirname "$file")"
    cp "$src/$file" "$dest/$file"
  done < <(cd "$src" && find . -type f)
}

# What the app reads from <dir>/assets at run time: the icons every theme draws by name, the app
# icon the start screen shows, and the default theme's preview in Settings (`runtime/app/app/assets.ts`).
copy_assets() {
  local dir="$1"
  [ -d "$REPO/assets/icons" ] || die "no icons at $REPO/assets/icons"
  rm -rf "$dir/assets"
  copy_tree "$REPO/assets/icons" "$dir/assets/icons"
  mkdir -p "$dir/assets/brand"
  cp "$REPO/assets/brand/app-icon-256.png" "$dir/assets/brand/app-icon-256.png"
  if [ -d "$REPO/assets/themes" ]; then copy_tree "$REPO/assets/themes" "$dir/assets/themes"; fi
}

# The .dSYMs beside the binaries in <built>, zipped at <zip>: kept apart from the app, to read
# a crash report's addresses with (`atos -o pumid.dSYM/... -l <load address> <address>`).
zip_symbols() {
  local built="$1" zip="$2" found=()
  shopt -s nullglob
  found=("$built"/*.dSYM)
  shopt -u nullglob
  [[ ${#found[@]} -gt 0 ]] || { say "no .dSYM in $built: the zip of symbols is skipped"; return 0; }
  rm -f "$zip"
  (cd "$built" && zip -qr "$zip" "${found[@]##*/}")
}

# The runtime directory, into <dir>/runtime: only what is read from disk at run time. The app
# itself (React, the SDKs, the workers) is compiled into the `pumi` executable, so runtime/app's
# sources, node_modules and dev addon stay out. What is read:
#   bundled/               the plugins that ship with pumi (`crates/pumi-host/src/plugins.rs`)
#   themes/                on the runtimepath
#   app/worker/do.ts, sdk/ what `pumi do` runs on the app's Bun (`crates/pumi-cli/src/run_do.rs`)
copy_runtime() {
  local dir="$1"
  rm -rf "$dir/runtime"
  copy_tree "$REPO/runtime/bundled" "$dir/runtime/bundled"
  copy_tree "$REPO/runtime/themes" "$dir/runtime/themes"
  copy_tree "$REPO/runtime/app/worker/sdk" "$dir/runtime/app/worker/sdk"
  for file in app/worker/do.ts app/worker/rpc.ts; do
    mkdir -p "$dir/runtime/$(dirname "$file")"
    cp "$REPO/runtime/$file" "$dir/runtime/$file"
  done
  mkdir -p "$dir/runtime/app" # pumi-host knows the runtime by its app/
  [[ -f "$dir/runtime/app/worker/do.ts" ]] || die "the runtime has no app/worker/do.ts: \`pumi do\` would not run"
  [[ -f "$dir/runtime/app/worker/rpc.ts" ]] || die "the runtime has no app/worker/rpc.ts: \`pumi do\` would not run"
  local junk
  junk="$(find "$dir/runtime" \( -name node_modules -o -name '*.node' -o -name target \) -print -quit)"
  [[ -z "$junk" ]] || die "the runtime carries $junk — build output, not the app"
}

# A zip of the bundle <src> at <zip>, as install.sh and the updater unpack it (ditto). No
# resource forks or extended attributes: those become a `._*` AppleDouble file per file
# (com.apple.provenance is on nearly everything), and the signature does not need them.
zip_bundle() {
  rm -f "$2"
  ditto -c -k --norsrc --noextattr --noacl --keepParent "$1" "$2"
}

# The same for anything but macOS: `-X` leaves out the attributes `ditto` exists to strip, and
# Windows' installer unpacks it with Expand-Archive. Git Bash on GitHub's Windows runner has no
# `zip`, but has 7-Zip, which writes none of those attributes.
zip_plain() {
  rm -f "$2"
  if command -v zip >/dev/null 2>&1; then
    (cd "$(dirname "$1")" && zip -qrX "$2" "$(basename "$1")")
  elif command -v 7z >/dev/null 2>&1; then
    (cd "$(dirname "$1")" && 7z a -tzip -bso0 -bsp0 "$2" "$(basename "$1")")
  else
    die "zip (or 7z) is missing: it packs the Windows artifact"
  fi
}

# The CLI, into <dir>/bin: `pumi-cli`, and `pumi` as its second name: a link, or a copy of it
# with <exe> appended, which is what Windows has instead of a link.
copy_cli() {
  local built="$1" dir="$2" exe="${3:-}"
  mkdir -p "$dir/bin"
  install -m 755 "$built/pumi-cli$exe" "$dir/bin/pumi-cli$exe"
  if [ -n "$exe" ]; then cp "$dir/bin/pumi-cli$exe" "$dir/bin/pumi$exe"; else ln -sf pumi-cli "$dir/bin/pumi"; fi
}
