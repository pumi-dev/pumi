#!/bin/sh
# pumi: from this folder into /Applications.
#
#   sh install.sh                    -> /Applications/pumi.app
#   DEST="$HOME/Applications" sh install.sh
#
# Nothing else to install: the terminal server (pumid) and the plugins this build ships with are
# inside the app bundle. Your settings go in ~/.config/pumi; removing the app leaves them there.
set -eu

APP=pumi
HERE=$(cd "$(dirname "$0")" && pwd)
SRC="$HERE/$APP.app"
VERSION="@VERSION@"
LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
warn() { printf '\033[1m!!\033[0m %s\n' "$*" >&2; }

[ -d "$SRC" ] || {
  warn "there is no $APP.app next to this script."
  echo "   Unzip the whole archive first, then run this from inside the folder it made." >&2
  exit 1
}

# /Applications, unless the account cannot write it: then this account's own Applications.
GIVEN=${DEST:-}
DEST=${DEST:-/Applications}
if ! mkdir -p "$DEST" 2>/dev/null || [ ! -w "$DEST" ]; then
  if [ -n "$GIVEN" ]; then
    warn "cannot write $DEST"
    exit 1
  fi
  DEST="$HOME/Applications"
  mkdir -p "$DEST" || {
    warn "cannot create $DEST"
    exit 1
  }
fi
APP_PATH="$DEST/$APP.app"
EXE="$APP_PATH/Contents/MacOS/$APP"

# Only the app is quit, never the terminal server beside it: it holds every terminal, and the
# app hands it over to its own binary when it starts.
if pgrep -f "^$EXE\$" >/dev/null 2>&1; then
  say "quitting the running copy"
  pkill -f "^$EXE\$" 2>/dev/null || true
  sleep 1
fi

say "installing to $APP_PATH"
# The copy is made beside the destination and renamed over it once it is whole: a copy that stops
# half way would leave a bundle macOS refuses to open, with the old one already gone.
STAGED="$DEST/.$APP.app.new"
OLD="$DEST/.$APP.app.old"
rm -rf "$STAGED" "$OLD"
if ! ditto "$SRC" "$STAGED"; then
  rm -rf "$STAGED"
  warn "the copy failed — nothing installed"
  exit 1
fi
# The flag a browser, Mail or AirDrop sets on the download is what makes macOS hold the first
# launch of an app it has never seen. It is not part of the app.
xattr -cr "$STAGED" 2>/dev/null || true
[ -e "$APP_PATH" ] && mv "$APP_PATH" "$OLD"
if ! mv "$STAGED" "$APP_PATH"; then
  [ -e "$OLD" ] && mv "$OLD" "$APP_PATH"
  warn "could not write $APP_PATH — nothing installed"
  exit 1
fi
rm -rf "$OLD"

if codesign --verify --deep --strict "$APP_PATH" >/dev/null 2>&1; then
  echo "    signature:   intact"
else
  echo "    signature:   not intact (the app still runs; it is not signed by Apple)"
fi
[ -x "$LSREGISTER" ] && "$LSREGISTER" -f "$APP_PATH" >/dev/null 2>&1 || true

# `pumi` in a plain shell too, from the copy that is installed. Inside the app's own tabs
# pumid puts the CLI first on PATH by itself.
mkdir -p "$HOME/.local/bin"
ln -sf "$APP_PATH/Contents/Resources/bin/pumi" "$HOME/.local/bin/pumi"
case ":$PATH:" in
  *":$HOME/.local/bin:"*) ;;
  *) echo "    (add ~/.local/bin to your PATH to run \`pumi\` from a shell)" ;;
esac

# A first install: this is a build to test, not a release, so it does not update itself over the
# official one. Nothing is written when settings are already there.
SETTINGS="$HOME/.config/pumi/settings.json"
if [ ! -f "$SETTINGS" ]; then
  mkdir -p "$(dirname "$SETTINGS")"
  cat >"$SETTINGS" <<'JSON'
{
  "options": {
    "autoupdate": false
  }
}
JSON
  echo "    updates:     off (Settings > Updates turns them on)"
fi

say "opening $APP_PATH"
open "$APP_PATH" 2>/dev/null || true
say "done — pumi $VERSION is in $APP_PATH"
