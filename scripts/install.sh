#!/usr/bin/env bash
set -euo pipefail

# pumi.app, from this checkout: the app (a Bun executable), its addon, the server and the CLI,
# signed, zipped for sending, and put in /Applications. It always builds first: what is installed
# is what this checkout says, never a build older than it.
REPO="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
# copy_runtime and zip_bundle: what goes in the bundle is the release's.
source "$REPO/scripts/release/common.sh"
APP=pumi
BUNDLE_ID=local.pumi
DEST="${DEST:-/Applications}"
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$REPO/Cargo.toml" | head -1)"
ZIP="$REPO/dist/$APP-$VERSION.zip"

say() { printf '\033[1m==>\033[0m %s\n' "$*"; }
fail() { echo "   !! $*" >&2; exit 1; }

# ── Signing ──────────────────────────────────────────────────────────
# An Apple-issued certificate first: hardened runtime only lets the app dlopen the addon when both
# carry the same Team ID, and a self-signed certificate ("pumi local signing") has none.
identity() { security find-identity -v -p codesigning 2>/dev/null | sed -n "s/.*\"\($1[^\"]*\)\".*/\1/p" | head -1; }
IDENTITY="${CODESIGN_IDENTITY:-$(identity 'Developer ID Application: ')}"
[[ -z "$IDENTITY" ]] && IDENTITY="$(identity 'Apple Development: ')"
[[ -z "$IDENTITY" ]] && IDENTITY="$(identity 'pumi local signing')"
SIGN_IDENTITY="${IDENTITY:--}"
if [[ "$IDENTITY" == Developer\ ID* ]]; then
  SIGN_KIND="Developer ID: $IDENTITY"; SIGN_FLAGS="--options runtime --timestamp"
elif [[ -n "$IDENTITY" ]]; then
  SIGN_KIND="local certificate: $IDENTITY"; SIGN_FLAGS="--options runtime"
else
  SIGN_KIND="ad-hoc (no certificate in the keychain; ./scripts/make-signing-cert.sh makes one)"; SIGN_FLAGS=""
fi
NOTARY_PROFILE="${NOTARY_PROFILE:-}"

# ── The build ────────────────────────────────────────────────────────
# The app is a Bun process: `bun run app` compiles it to target/pumi, with its addon (the root
# crate) beside it, `target/pumi.<platform>-<arch>.node`, which it dlopens from there.
say "building the app, its addon, the server and the shims (release)"
( cd "$REPO/runtime/app" && bun run addon --release && bun run app )
( cd "$REPO" && cargo build --release -p pumid -p pumi-host -p pumi-cli )

# What is installed has to be this checkout, compiled after the last edit: a `target/pumi` older
# than the code it carries is what "it did not update" looks like, and nothing later notices.
say "checking the build is of this checkout"
STALE="$(find "$REPO/runtime/app/app" "$REPO/runtime/app/worker" "$REPO/runtime/app/sdk" \
  "$REPO/runtime/app/react" "$REPO/runtime/app/build" -type f -newer "$REPO/target/pumi" 2>/dev/null | head -3)"
[[ -n "$STALE" ]] && fail "target/pumi is older than its own sources ($(echo $STALE)): the app did not rebuild"
for binary in pumid pumi-host pumi-cli; do
  # Only the package's own sources: `crates` also holds packages this binary is not built from
  # (`cargo` rebuilds a dependent itself), and tests are not in the binary at all.
  STALE="$(find "$REPO/crates/$binary/src" -name '*.rs' -newer "$REPO/target/release/$binary" 2>/dev/null | head -1)"
  [[ -n "$STALE" ]] && fail "target/release/$binary is older than $STALE: it did not rebuild"
done

# ── Assembling (beside the destination, never in dist/) ──────────────
# An app left in dist/pumi.app is registered by macOS as a second `local.pumi`: the Dock could
# open that copy instead of the installed one. Nothing is kept.
if [[ -e "$REPO/dist/$APP.app" ]]; then
  say "removing the old dist copy (a second app macOS could open)"
  LSREGISTER=/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister
  [[ -x "$LSREGISTER" ]] && "$LSREGISTER" -u "$REPO/dist/$APP.app" 2>/dev/null || true
  rm -rf "$REPO/dist/$APP.app"
fi

STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT
BUNDLE="$STAGE/$APP.app"
say "assembling $APP.app"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources/bin"

# The addon beside the compiled app: `pumi.<platform>-<arch>.node`, as `process` names them
# (`darwin-arm64`, `darwin-x64`), because the app dlopens it from its own directory.
ARCH="$(uname -m)"; [[ "$ARCH" == x86_64 ]] && ARCH=x64
ADDON="pumi.darwin-$ARCH.node"
cp "$REPO/target/pumi" "$BUNDLE/Contents/MacOS/pumi"
cp "$REPO/target/$ADDON" "$BUNDLE/Contents/MacOS/$ADDON"
cp "$REPO/target/release/pumid" "$REPO/target/release/pumi-host" "$BUNDLE/Contents/MacOS/"
cp "$REPO/target/release/pumi-cli" "$BUNDLE/Contents/Resources/bin/pumi-cli"
ln -s pumi-cli "$BUNDLE/Contents/Resources/bin/pumi"

copy_assets "$BUNDLE/Contents/Resources"
copy_runtime "$BUNDLE/Contents/Resources"

say "rendering the app icon"
ICONSET="$STAGE/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$REPO/assets/brand/app-icon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  sips -z "$((size * 2))" "$((size * 2))" "$REPO/assets/brand/app-icon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/AppIcon.icns"

say "writing Info.plist"
cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP</string>
  <key>CFBundleDisplayName</key><string>$APP</string>
  <key>CFBundleExecutable</key><string>pumi</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>1</string>
  <!-- The About panel (pumi → About pumi): AppKit reads the name, the version and this line. -->
  <key>NSHumanReadableCopyright</key><string>Copyright © 2026 the pumi authors. GPL-3.0.</string>
  <key>LSMinimumSystemVersion</key><string>14.0</string>
  <key>LSApplicationCategoryType</key><string>public.app-category.developer-tools</string>
  <key>NSHighResolutionCapable</key><true/>
  <!-- Files and folders dropped on the Dock icon, or opened with pumi: never their default app. -->
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key><string>Files and Folders</string>
      <key>CFBundleTypeRole</key><string>Viewer</string>
      <key>LSHandlerRank</key><string>Alternate</string>
      <key>LSItemContentTypes</key><array><string>public.item</string></array>
    </dict>
  </array>
  <!-- pumi://settings/<path>: a setting's link, copied from its row in Settings. -->
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key><string>$BUNDLE_ID</string>
      <key>CFBundleURLSchemes</key><array><string>pumi</string></array>
    </dict>
  </array>
</dict>
</plist>
PLIST

say "third-party notices"
copy_notices "$BUNDLE/Contents/Resources"

BUNDLE_MB=$(( $(du -sm "$BUNDLE" | cut -f1) ))
(( BUNDLE_MB > 200 )) && fail "$APP.app is ${BUNDLE_MB} MB, too big for a production build; its largest parts: $(du -sm "$BUNDLE/Contents/Resources"/* | sort -n | tail -3 | tr '\n' ' ')"

# Extended attributes (provenance, Finder info) are not the app: codesign refuses some, and the
# zip would carry each as a `._*` file.
xattr -cr "$BUNDLE" 2>/dev/null || true
say "signing ($SIGN_KIND)"
# pumi-host keeps its own dialogs' libraries loadable; the addon and every other nested Mach-O
# must carry the same Team ID as the app, or dyld refuses it ("different Team IDs").
ENTITLEMENTS="$STAGE/pumid-entitlements.plist"
cat > "$ENTITLEMENTS" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
PLIST
# The app is Bun: JSC compiles code as it runs, and under the hardened runtime that needs
# `allow-jit`. Without it the engine silently loses `SharedArrayBuffer`, which the worker the
# windows run in is built on (`native/bridge-host.ts`), and the app dies as it starts.
APP_ENTITLEMENTS="$STAGE/pumi-entitlements.plist"
cat > "$APP_ENTITLEMENTS" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.allow-jit</key>
  <true/>
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
PLIST
codesign --force $SIGN_FLAGS --entitlements "$ENTITLEMENTS" --sign "$SIGN_IDENTITY" \
  "$BUNDLE/Contents/MacOS/pumi-host" >/dev/null 2>&1 || echo "   (codesign failed for pumi-host)"
for helper in "$BUNDLE/Contents/MacOS/pumid" "$BUNDLE/Contents/MacOS/$ADDON" "$BUNDLE/Contents/Resources/bin/pumi-cli"; do
  codesign --force $SIGN_FLAGS --sign "$SIGN_IDENTITY" "$helper" >/dev/null 2>&1 \
    || echo "   (codesign failed for $(basename "$helper"))"
done
codesign --force $SIGN_FLAGS --entitlements "$APP_ENTITLEMENTS" --sign "$SIGN_IDENTITY" "$BUNDLE" >/dev/null 2>&1 \
  || echo "   (codesign failed; the app still runs locally)"

# ── The zip, and the note that goes with it ──────────────────────────
mkdir -p "$REPO/dist"
make_zip() { zip_bundle "$BUNDLE" "$ZIP"; }
if [[ -n "$NOTARY_PROFILE" && "$SIGN_KIND" == Developer\ ID* ]]; then
  make_zip
  say "notarizing (profile $NOTARY_PROFILE)"
  if xcrun notarytool submit "$ZIP" --keychain-profile "$NOTARY_PROFILE" --wait; then
    xcrun stapler staple "$BUNDLE"
    say "notarized and stapled — this copy runs anywhere with no warnings"
  else
    echo "   (notarization failed; the copy is signed but not notarized)"
  fi
fi
make_zip
say "zipped for sending: $ZIP"
shasum -a 256 "$ZIP" | awk -v f="$(basename "$ZIP")" '{print $1 "  " f}' > "$ZIP.sha256"

cat > "$REPO/dist/SEND-README.md" <<README
# Send this to a friend

Attach \`$(basename "$ZIP")\` (about $(( $(stat -f%z "$ZIP") / 1024 / 1024 )) MB). It is
\`pumi\` $VERSION, built for **$(lipo -info "$BUNDLE/Contents/MacOS/pumi" 2>/dev/null | sed 's/.*: //')**.

1. Unzip it and drag \`pumi.app\` into \`/Applications\`.

2. **If macOS offers to Move it to the Bin, click Done — do not click Move to Bin.** That
   dialog is macOS failing to verify an app it has never seen; it is not a malware warning,
   and the app is fine. The wording and buttons are Apple's — no app can change them — so
   here is how to get past it, best first:

   - **Copy it from a USB stick, or over \`scp\`/\`rsync\`.** Files that arrive that way are not
     quarantined at all, and the app opens normally.
   - **Or clear the flag** (Terminal, one line):
     \`\`\`
     xattr -dr com.apple.quarantine "/Applications/pumi.app"
     \`\`\`
   - **Or** try to open it once, then System Settings > Privacy & Security > **Open Anyway**
     and confirm. (macOS 15 removed the old right-click-to-open bypass.)

   The dialog only never appears if the app is signed with an Apple Developer ID and
   notarized; this copy is ${SIGN_KIND}.

3. Nothing else to install: the terminal server (\`pumid\`) ships inside the app and starts
   by itself. Terminals live there, so they keep running when the app quits; the \`pumi\`
   CLI in every terminal talks to it.
README
say "note for them: $REPO/dist/SEND-README.md"
if [[ "$SIGN_KIND" == ad-hoc* ]]; then
  cat <<'SEND'
   Ad-hoc signed: on another Mac Gatekeeper holds the first launch until one of
   * run: xattr -dr com.apple.quarantine "/Applications/pumi.app"   (reliable), or
   * System Settings > Privacy & Security > "Open Anyway"   (macOS 15 dropped right-click)
   For a copy that just opens, sign with a Developer ID Application certificate:
     CODESIGN_IDENTITY="Developer ID Application: You (TEAMID)" \
       NOTARY_PROFILE=notary-profile ./scripts/install.sh
SEND
fi

# ── Installing ───────────────────────────────────────────────────────
PROCESS="^$DEST/$APP.app/Contents/MacOS/pumi( |\$)"
if pgrep -f "$PROCESS" >/dev/null 2>&1; then
  say "quitting the running copy"
  pkill -f "$PROCESS" || true
  sleep 1
fi

say "installing to $DEST/$APP.app"
mkdir -p "$DEST"
NEED_MB=$(( $(du -sm "$BUNDLE" | cut -f1) ))
FREE_MB=$(( $(df -m "$DEST" | awk 'NR == 2 { print $4 }') ))
(( FREE_MB < NEED_MB )) && fail "$DEST has ${FREE_MB} MB free, the app needs ${NEED_MB} MB — nothing installed"

# The copy is made beside the destination and renamed over it once it is whole: a copy that stops
# half way (a full disk, a Ctrl-C) would leave a bundle macOS refuses to open ('The application
# "pumi" can't be opened'), with the old one already gone. `ditto` keeps the signature and the
# symlinks exactly, as making the zip does.
STAGED="$DEST/.$APP.app.new"
rm -rf "$STAGED"
ditto "$BUNDLE" "$STAGED"
CAME=$(find "$STAGED" -type f | wc -l | tr -d ' ')
WENT=$(find "$BUNDLE" -type f | wc -l | tr -d ' ')
[[ "$CAME" == "$WENT" ]] || { rm -rf "$STAGED"; fail "the copy holds $CAME of $WENT files — nothing installed"; }
# The programs, byte for byte: a copy that kept an older `pumi` (or addon) is what a build that
# "did not apply" looks like from the outside.
for program in pumi "$ADDON" pumid pumi-host; do
  WANT="$(shasum -a 256 "$BUNDLE/Contents/MacOS/$program" | cut -d' ' -f1)"
  GOT="$(shasum -a 256 "$STAGED/Contents/MacOS/$program" | cut -d' ' -f1)"
  [[ "$WANT" == "$GOT" ]] || { rm -rf "$STAGED"; fail "$program did not copy whole — nothing installed"; }
done
xattr -cr "$STAGED"
codesign --verify --deep --strict "$STAGED" >/dev/null 2>&1 \
  || { rm -rf "$STAGED"; fail "the copy is not signed as $BUNDLE is — nothing installed"; }
echo "    signature:   valid on disk (and whole: $CAME files)"

# Two renames inside one directory: the installed app is only ever the old bundle or the new one.
OLD="$DEST/.$APP.app.old"
rm -rf "$OLD"
[[ -e "$DEST/$APP.app" ]] && mv "$DEST/$APP.app" "$OLD"
mv "$STAGED" "$DEST/$APP.app"
rm -rf "$OLD"

if spctl -a --type execute "$DEST/$APP.app" >/dev/null 2>&1; then
  echo "    gatekeeper:  accepted"
else
  echo "    gatekeeper:  rejected (unsigned or unnotarized: first launch asks the user)"
fi

# ── Updating what runs ───────────────────────────────────────────────
# The installed bundle is new, but the app was quit above and the core (`pumid`, and the host
# beside it) still runs the images it started with — the binaries under them were only replaced
# on disk. `--restart` hands every terminal and the program in it to the binaries now on disk
# (the same process image, so nothing is lost), and the app opens again: an update that leaves
# the old app or the old server running is what "it did not update" means.
say "restarting the server on the new binaries"
if "$DEST/$APP.app/Contents/MacOS/pumid" --restart >/dev/null 2>&1; then
  echo "    server:      restarted (terminals kept)"
else
  echo "    server:      not running (the app starts one)"
fi
say "opening $DEST/$APP.app"
open "$DEST/$APP.app"
sleep 2
RUNNING=$(pgrep -f "^$DEST/$APP.app/Contents/MacOS/pumi( |\$)" | head -1)
if [[ -n "$RUNNING" ]]; then echo "    running:     $RUNNING"; else echo "    running:     not yet (macOS may still be checking the new copy)"; fi

say "done — pumi $VERSION at $DEST/$APP.app"
echo "    app built:   $(date -r "$(stat -f %m "$BUNDLE/Contents/MacOS/pumi")" '+%Y-%m-%d %H:%M:%S')"
echo "    settings:    ~/.config/pumi/settings.json (shared with the dev build)"
