#!/bin/sh
# pumi — the installer, for macOS (Apple silicon and Intel) and Linux (x86_64 and arm64).
#
#   curl -fsSL https://pumi.dev/install.sh | sh
#   curl -fsSL https://pumi.dev/install.sh | sh -s -- --version 0.42.0
#   curl -fsSL https://pumi.dev/install.sh | sh -s -- --uninstall
#
# Windows has its own, in PowerShell: `irm https://pumi.dev/install.ps1 | iex`
# (scripts/release/install.ps1). This one stops on Git-Bash and says so.
#
# Or straight from the repository:
#
#   curl -fsSL https://raw.githubusercontent.com/pumi-dev/pumi/main/scripts/release/install.sh | sh
#
# Why a script and not a download button (macOS): Gatekeeper only ever looks at files that
# carry `com.apple.quarantine`, and a browser sets that flag while curl does not. So nothing
# this script downloads is quarantined, and pumi opens with no "Open Anyway" trip through
# System Settings, without an Apple Developer ID. The app's updater downloads the same way.
#
# What is checked: the release manifest's minisign signature (when `minisign` is installed;
# otherwise only HTTPS vouches for it), and the archive's SHA-256 against that manifest.
#
# Env:
#   PUMI_VERSION=0.42.0        install this version instead of the newest
#   PUMI_DIR=<dir>            where the app goes (macOS: /Applications, else ~/Applications;
#                             Linux: ~/.local)
#   PUMI_GITHUB_TOKEN=<tok>   for a release that is not public; kept for the app's updater
#   PUMI_NO_OPEN=1            do not launch pumi when done
#   PUMI_REPO, PUMI_SITE      where releases and the manifest live

set -eu

APP="pumi"
REPO="${PUMI_REPO:-pumi-dev/pumi}"
SITE="${PUMI_SITE:-https://pumi.dev}"
CHANNEL="preview"
APP_ID="dev.pumi.app"
# The update keys (src/updater/manifest.rs): the primary CI signs with, and the backup.
KEYS="RWQGWJa1jK6fXLhk2t8/zrWDojiRPRykE5DkbUI0/xcdWcU9YfczkJ79 RWQe/SSsIy1UiLqkZRlz4FJoUwBTAkRbMcEgsddiFiXHblnemzQvvQ2K"

say()  { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1m==>\033[0m \033[33m%s\033[0m\n' "$*" >&2; }
die()  { printf '\033[1m==>\033[0m \033[31m%s\033[0m\n' "$*" >&2; exit 1; }

usage() {
  cat <<'EOF'
pumi installer

  curl -fsSL https://pumi.dev/install.sh | sh

Options
  --version <x.y.z>   install a specific version
  --dir <path>        where pumi goes (macOS: /Applications; Linux: ~/.local)
  --bin               (macOS) also link `pumi` into ~/.local/bin, so it works outside pumi's tabs
                      (Linux always links it)
  --no-open           do not launch pumi when it is installed
  --uninstall         remove pumi (settings in ~/.config/pumi are left alone)
  -h, --help

Env: PUMI_VERSION, PUMI_DIR, PUMI_GITHUB_TOKEN, PUMI_NO_OPEN
EOF
}

# ---- arguments -------------------------------------------------------------

VERSION="${PUMI_VERSION:-}"
DIR="${PUMI_DIR:-}"
WANT_BIN=0
OPEN=1
UNINSTALL=0
[ -z "${PUMI_NO_OPEN:-}" ] || OPEN=0

while [ $# -gt 0 ]; do
  case "$1" in
    --version) [ $# -ge 2 ] || die "--version wants a value"; VERSION="$2"; shift 2 ;;
    --version=*) VERSION="${1#*=}"; shift ;;
    --dir) [ $# -ge 2 ] || die "--dir wants a value"; DIR="$2"; shift 2 ;;
    --dir=*) DIR="${1#*=}"; shift ;;
    --bin) WANT_BIN=1; shift ;;
    --no-open) OPEN=0; shift ;;
    --uninstall) UNINSTALL=1; shift ;;
    -h|--help) usage; exit 0 ;;
    *) die "unknown option: $1 (try --help)" ;;
  esac
done
VERSION="${VERSION#v}"

# ---- what this machine is --------------------------------------------------

OS="$(uname -s)"
case "$OS" in
  Darwin)
    OS=macos
    # The hardware, not the shell: a Terminal running under Rosetta still gets the native build.
    if [ "$(sysctl -n hw.optional.arm64 2>/dev/null || echo 0)" = 1 ]; then ARCH=aarch64; else ARCH=x86_64; fi
    OS_VERSION="$(sw_vers -productVersion)"
    [ "${OS_VERSION%%.*}" -ge 14 ] 2>/dev/null || die "pumi needs macOS 14 or later; this Mac runs $OS_VERSION."
    ;;
  Linux)
    OS=linux
    case "$(uname -m)" in
      x86_64|amd64) ARCH=x86_64 ;;
      aarch64|arm64) ARCH=aarch64 ;;
      *) die "pumi is built for x86_64 and arm64 Linux; this is $(uname -m)." ;;
    esac
    if ldd --version 2>&1 | grep -qi musl; then
      die "pumi's Linux build needs glibc (Debian, Ubuntu, Fedora, Arch, …); this system uses musl."
    fi
    # Built on Ubuntu 22.04: an older glibc unpacks fine, then the app fails to start.
    glibc="$(getconf GNU_LIBC_VERSION 2>/dev/null | awk '{print $2}')"
    case "$glibc" in
      2.*)
        minor="${glibc#2.}"; minor="${minor%%.*}"
        [ "$minor" -ge 35 ] 2>/dev/null \
          || die "pumi needs glibc 2.35 or later (Debian 12, Ubuntu 22.04, Fedora 36); this system has $glibc."
        ;;
    esac
    ;;
  MINGW*|MSYS*|CYGWIN*)
    die "this is Git-Bash (or another shell on Windows): pumi's Windows installer is PowerShell's,
   irm https://pumi.dev/install.ps1 | iex"
    ;;
  *) die "pumi runs on macOS, Linux and Windows; this is $OS." ;;
esac
PLATFORM="$OS-$ARCH"

command -v curl >/dev/null 2>&1 || die "curl is missing."

if [ "$OS" = macos ]; then
  dest="${DIR:-/Applications}"
  if [ ! -w "$dest" ] && [ -z "$DIR" ]; then
    dest="$HOME/Applications"
  fi
  APP_PATH="$dest/$APP.app"
  EXE="$APP_PATH/Contents/MacOS/$APP"
  CLI="$APP_PATH/Contents/Resources/bin/pumi-cli"
else
  dest="${DIR:-$HOME/.local}"
  APP_PATH="$dest/$APP.app"
  EXE="$APP_PATH/bin/$APP"
  CLI="$APP_PATH/Resources/bin/pumi-cli"
fi
LINK="$HOME/.local/bin/$APP"
DATA="${XDG_DATA_HOME:-$HOME/.local/share}"
# Where the updater reads its token on Linux (pumi-sys config_dir).
CONFIG="${XDG_CONFIG_HOME:-$HOME/.config}/$APP"

# ---- uninstall -------------------------------------------------------------

if [ "$UNINSTALL" = 1 ]; then
  say "removing $APP_PATH"
  rm -rf "$APP_PATH" "$dest/.$APP.app.old"
  [ -L "$LINK" ] && rm -f "$LINK" && say "removed $LINK"
  if [ "$OS" = linux ]; then
    rm -f "$DATA/applications/$APP_ID.desktop"
    rm -f "$DATA"/icons/hicolor/*/apps/"$APP_ID".png
  else
    security delete-generic-password -s dev.pumi.github-token >/dev/null 2>&1 || true
  fi
  echo "    settings and plugin data are still in ~/.config/pumi"
  exit 0
fi

# ---- where the release is ---------------------------------------------------

TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
trap 'rm -rf "$TMP"; exit 130' INT TERM

TOKEN="${PUMI_GITHUB_TOKEN:-}"
if [ -z "$TOKEN" ] && command -v gh >/dev/null 2>&1; then
  TOKEN="$(gh auth token 2>/dev/null || true)"
fi

# fetch <url> <out> [accept]: the token goes only to GitHub's API, and through a curl config
# on stdin rather than argv.
fetch() {
  {
    printf 'url = "%s"\noutput = "%s"\n' "$1" "$2"
    printf 'header = "User-Agent: pumi-installer"\n'
    [ -n "${3:-}" ] && printf 'header = "Accept: %s"\n' "$3"
    case "$1" in
      https://api.github.com/*) [ -n "$TOKEN" ] && printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" ;;
    esac
    true
  } | curl --config - -fsSL --proto '=https' --retry 3 --retry-delay 1
}

MODE=""
# github_manifest: the release's own JSON, from GitHub's API (a version, or the newest).
github_manifest() {
  if [ -n "$VERSION" ]; then api="https://api.github.com/repos/$REPO/releases/tags/v$VERSION"
  else api="https://api.github.com/repos/$REPO/releases/latest"; fi
  fetch "$api" "$TMP/release.json" "application/vnd.github+json" 2>/dev/null || {
    if [ -z "$TOKEN" ]; then
      die "could not find a pumi release${VERSION:+ $VERSION}. If the repository is private, set
   PUMI_GITHUB_TOKEN (a token that can read $REPO), or log in with \`gh auth login\`."
    fi
    die "could not find a pumi release${VERSION:+ $VERSION} in $REPO."
  }
  MODE=github
}
# The newest release's manifest, published on the site.
if [ -z "$VERSION" ] && fetch "$SITE/releases/$CHANNEL/latest.json" "$TMP/latest.json" 2>/dev/null \
  && fetch "$SITE/releases/$CHANNEL/latest.json.minisig" "$TMP/latest.json.minisig" 2>/dev/null \
  && head -c 1 "$TMP/latest.json" | grep -q '{'; then
  MODE=site
else
  github_manifest
fi

# asset_api <name>: a release asset's API URL, from the release JSON (pretty-printed, one
# key per line, each asset's "url" before its "name").
asset_api() {
  awk -v want="$1" '
    /"url": "https:\/\/api\.github\.com\/repos\/.*\/releases\/assets\// { sub(/.*"url": "/, ""); sub(/".*/, ""); url = $0 }
    /"name": / { line = $0; sub(/.*"name": "/, "", line); sub(/".*/, "", line); if (line == want && url != "") { print url; exit } }
  ' "$TMP/release.json"
}

# get <name> <out> [url]: a release file, from its public URL or through the API.
get() {
  if [ "$MODE" = site ] && [ -n "${3:-}" ]; then
    fetch "$3" "$2"
  else
    url="$(asset_api "$1")"
    [ -n "$url" ] || return 1
    fetch "$url" "$2" "application/octet-stream"
  fi
}

if [ "$MODE" = github ]; then
  get latest.json "$TMP/latest.json" || die "the release has no latest.json"
  get latest.json.minisig "$TMP/latest.json.minisig" || die "the release has no latest.json.minisig"
fi

# ---- prove what we got -----------------------------------------------------

# signed: the manifest is signed by one of the release keys.
signed() {
  for key in $KEYS; do
    if out="$(minisign -V -m "$TMP/latest.json" -x "$TMP/latest.json.minisig" -P "$key" 2>&1)" \
      && printf '%s' "$out" | grep -q 'file:latest.json'; then
      return 0
    fi
  done
  return 1
}

if command -v minisign >/dev/null 2>&1; then
  # The site caches the manifest and its signature apart: just after a release they can be of
  # two releases for a few minutes. GitHub's pair is one release's.
  if ! signed && [ "$MODE" = site ]; then
    github_manifest
    get latest.json "$TMP/latest.json" || die "the release has no latest.json"
    get latest.json.minisig "$TMP/latest.json.minisig" || die "the release has no latest.json.minisig"
  fi
  signed || die "the release manifest is not signed by a pumi release key."
  say "signature:   the manifest is signed by pumi's release key"
else
  warn "signature:   not checked (install minisign to check it); trusting HTTPS"
fi

# field <platform> <key>: one value of one asset in the manifest (pretty-printed by
# scripts/release/manifest.sh: one key per line).
field() {
  awk -v platform="\"$1\": {" -v key="\"$2\": " '
    index($0, platform) { inside = 1; next }
    inside && /}/ { exit }
    inside && index($0, key) { line = $0; sub(/^[^:]*: /, "", line); gsub(/[",]/, "", line); print line; exit }
  ' "$TMP/latest.json"
}
manifest_version="$(sed -n 's/^  "version": "\([^"]*\)".*/\1/p' "$TMP/latest.json" | head -1)"
NAME="$(field "$PLATFORM" name)"
SHA="$(field "$PLATFORM" sha256)"
URL="$(field "$PLATFORM" url)"
[ -n "$NAME" ] && [ -n "$SHA" ] || die "pumi $manifest_version has no build for $PLATFORM."

# ---- fetch -----------------------------------------------------------------

say "downloading pumi $manifest_version for $PLATFORM"
get "$NAME" "$TMP/$NAME" "$URL" || die "download failed: $NAME"
if command -v shasum >/dev/null 2>&1; then have="$(shasum -a 256 "$TMP/$NAME" | awk '{print $1}')"
else have="$(sha256sum "$TMP/$NAME" | awk '{print $1}')"; fi
[ "$have" = "$SHA" ] || die "the download does not match the signed manifest.
   expected $SHA
   got      $have"
say "checksum:    matches the manifest"

# ---- install ---------------------------------------------------------------

mkdir -p "$TMP/unpacked"
if [ "$OS" = macos ]; then
  # ditto, as the zip was made: it restores the signature and symlinks exactly.
  ditto -x -k "$TMP/$NAME" "$TMP/unpacked"
else
  tar -xzf "$TMP/$NAME" -C "$TMP/unpacked"
fi
[ -d "$TMP/unpacked/$APP.app" ] || die "the download holds no $APP.app"

# Linux links the libraries of a desktop (GTK, WebKitGTK, xkbcommon, Vulkan's loader) through the
# addon, and a server install may have none. Say what to install before pumi fails to start.
if [ "$OS" = linux ]; then
  missing="$(ldd "$TMP/unpacked/$APP.app"/bin/$APP.linux-*.node "$TMP/unpacked/$APP.app"/bin/$APP 2>/dev/null \
    | awk '/not found/ { print $1 }' | sort -u | tr '\n' ' ')"
  if [ -n "$missing" ]; then
    warn "the app is missing libraries: $missing"
    echo "    Debian/Ubuntu: sudo apt install libwebkit2gtk-4.1-0 libgtk-3-0 libxkbcommon0 libvulkan1"
    echo "    Fedora:        sudo dnf install webkit2gtk4.1 gtk3 libxkbcommon vulkan-loader"
    echo "    Arch:          sudo pacman -S webkit2gtk-4.1 gtk3 libxkbcommon vulkan-icd-loader"
  fi
fi

# Only the app is quit, never the terminal server (`pumid`) beside it: it holds every
# terminal, and the new app hands it over to its own binary.
if pgrep -f "^$EXE\$" >/dev/null 2>&1; then
  say "quitting the running copy"
  pkill -f "^$EXE\$" 2>/dev/null || true
  sleep 1
fi

say "installing to $APP_PATH"
mkdir -p "$dest" || die "cannot create $dest"
rm -rf "$dest/.$APP.app.old"
[ -e "$APP_PATH" ] && { mv "$APP_PATH" "$dest/.$APP.app.old" 2>/dev/null || die "could not replace $APP_PATH.
   macOS guards other apps' bundles with App Management permission:
   System Settings → Privacy & Security → App Management → enable your terminal,
   then run this again."; }
if ! mv "$TMP/unpacked/$APP.app" "$APP_PATH"; then
  [ -e "$dest/.$APP.app.old" ] && mv "$dest/.$APP.app.old" "$APP_PATH"
  die "could not write $APP_PATH"
fi
rm -rf "$dest/.$APP.app.old"

if [ "$OS" = macos ]; then
  # Nothing above set the quarantine flag, but a copy a browser put here earlier would still
  # carry it, and it is what makes Gatekeeper hold the first launch.
  xattr -cr "$APP_PATH" 2>/dev/null || true
  if codesign --verify --deep --strict "$APP_PATH" >/dev/null 2>&1; then
    say "signature:   the app's seal is intact"
  else
    warn "signature:   INVALID — this copy may not launch"
  fi
  /System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister \
    -f "$APP_PATH" >/dev/null 2>&1 || true
  # The updater reads the token from here while the repository is private.
  if [ -n "${PUMI_GITHUB_TOKEN:-}" ]; then
    security add-generic-password -U -s dev.pumi.github-token -a pumi -w "$PUMI_GITHUB_TOKEN" >/dev/null 2>&1 \
      && say "token:       kept in the login keychain for updates"
  fi
else
  # The desktop entry, its icon, and `pumi://` links. Exec is quoted as the desktop entry spec
  # asks (a path may hold spaces), and escaped for sed's replacement.
  mkdir -p "$DATA/applications"
  exec_line="\"$(printf '%s' "$EXE" | sed 's/["`$\\]/\\\\&/g')\""
  exec_sed="$(printf '%s' "$exec_line" | sed 's/[\\#&]/\\&/g')"
  sed "s#@EXEC@#$exec_sed#" "$APP_PATH/share/applications/$APP_ID.desktop" > "$DATA/applications/$APP_ID.desktop"
  for icon in "$APP_PATH"/share/icons/hicolor/*/apps/"$APP_ID".png; do
    size="$(basename "$(dirname "$(dirname "$icon")")")"
    mkdir -p "$DATA/icons/hicolor/$size/apps"
    cp "$icon" "$DATA/icons/hicolor/$size/apps/"
  done
  command -v update-desktop-database >/dev/null 2>&1 && update-desktop-database "$DATA/applications" 2>/dev/null || true
  command -v gtk-update-icon-cache >/dev/null 2>&1 && gtk-update-icon-cache -q "$DATA/icons/hicolor" 2>/dev/null || true
  command -v xdg-mime >/dev/null 2>&1 && xdg-mime default "$APP_ID.desktop" x-scheme-handler/pumi 2>/dev/null || true
  if [ -n "${PUMI_GITHUB_TOKEN:-}" ]; then
    mkdir -p "$CONFIG"
    (umask 077 && printf '%s\n' "$PUMI_GITHUB_TOKEN" > "$CONFIG/github-token")
    say "token:       kept in $CONFIG/github-token for updates"
  fi
  WANT_BIN=1
fi

if [ "$WANT_BIN" = 1 ]; then
  mkdir -p "$(dirname "$LINK")"
  ln -sf "$CLI" "$LINK"
  say "linked $LINK"
  case ":$PATH:" in
    *":$HOME/.local/bin:"*) ;;
    *) echo "    (add ~/.local/bin to your PATH to run \`pumi\` from a shell)" ;;
  esac
fi

say "pumi $manifest_version is installed"
echo "    app:      $APP_PATH"
if [ "$OS" = macos ]; then echo "    settings: ~/.config/pumi/settings.json"
else echo "    settings: $CONFIG/settings.json"; fi
echo "    updates:  automatic — pumi tells you when one is ready (Settings → General → Updates)"
echo "    docs:     $SITE/docs/"

if [ "$OPEN" = 1 ]; then
  if [ "$OS" = macos ]; then
    open "$APP_PATH"
  elif [ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]; then
    nohup "$EXE" >/dev/null 2>&1 &
  fi
fi
