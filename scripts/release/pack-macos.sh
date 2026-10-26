#!/usr/bin/env bash
#
# Assemble, sign and zip pumi.app for one Mac architecture, from what build.sh built.
#
#   scripts/release/pack-macos.sh aarch64          # -> $OUT/pumi-<version>-macos-aarch64.zip
#   NO_ZIP=1 scripts/release/pack-macos.sh aarch64 # just the bundle, at $OUT/macos-aarch64/pumi.app
#
# Signing (`SIGN_IDENTITY`):
#   unset / "-"     ad-hoc: runs here, but its identity changes with every build, so macOS asks
#                   for file and keychain permissions again after each update.
#   <SHA-1 | name>  a certificate in the keychain (`SIGN_KEYCHAIN` to name one). Releases use a
#                   self-signed certificate that never changes: the designated requirement is
#                   then `identifier <id> and certificate root = H"<sha1>"`, the same for every
#                   release, so permissions survive updates. It is not Apple's, so it does
#                   nothing for Gatekeeper — install.sh and the updater never set quarantine,
#                   which is what keeps Gatekeeper out of it.
#   A "Developer ID Application" identity gets the hardened runtime and a secure timestamp
#   as well, so the result can be notarized.
#
# BUNDLE_ID defaults to the release id; scripts/pack-friend.sh passes the dev one.

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

ARCH="${1:?usage: pack-macos.sh <aarch64|x86_64>}"
TRIPLE="$ARCH-apple-darwin"
BUNDLE_ID="${BUNDLE_ID:-dev.pumi.app}"
# BUILT_DIR: where the binaries are, if not cargo's per-target dir (the dev install's
# plain `cargo build` puts them in target/release).
BUILT="${BUILT_DIR:-$TARGET_DIR/$TRIPLE/release}"
[[ -x "$BUILT/pumi" ]] || die "no build of pumi for $TRIPLE (run scripts/release/build.sh $TRIPLE)"

STAGE="$OUT/macos-$ARCH"
BUNDLE="$STAGE/$APP_NAME.app"
ZIP="$OUT/$APP_NAME-$VERSION-macos-$ARCH.zip"

say "assembling $BUNDLE"
rm -rf "$BUNDLE"
mkdir -p "$BUNDLE/Contents/MacOS" "$BUNDLE/Contents/Resources"

# pumid finds pumi-host beside itself (`crates/pumid/src/lifecycle.rs`), and puts
# Resources/bin, where `pumi` is, first in every tab's PATH.
# The app dlopens its addon from its own directory, by the name `process` gives it.
ADDON="$(addon_name "$TRIPLE")"
for bin in pumi "$ADDON" pumid pumi-host; do
  cp "$BUILT/$bin" "$BUNDLE/Contents/MacOS/$bin"
done
copy_cli "$BUILT" "$BUNDLE/Contents/Resources"
copy_runtime "$BUNDLE/Contents/Resources"
copy_assets "$BUNDLE/Contents/Resources"
copy_notices "$BUNDLE/Contents/Resources"

say "rendering the app icon"
ICONSET="$(mktemp -d)/AppIcon.iconset"
mkdir -p "$ICONSET"
for size in 16 32 128 256 512; do
  sips -z "$size" "$size" "$REPO/assets/brand/app-icon.png" --out "$ICONSET/icon_${size}x${size}.png" >/dev/null
  double=$((size * 2))
  sips -z "$double" "$double" "$REPO/assets/brand/app-icon.png" --out "$ICONSET/icon_${size}x${size}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$BUNDLE/Contents/Resources/AppIcon.icns"
rm -rf "$(dirname "$ICONSET")"

cat > "$BUNDLE/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>$APP_NAME</string>
  <key>CFBundleDisplayName</key><string>$APP_NAME</string>
  <key>CFBundleExecutable</key><string>pumi</string>
  <key>CFBundleIdentifier</key><string>$BUNDLE_ID</string>
  <key>CFBundleIconFile</key><string>AppIcon</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleVersion</key><string>$BUILD_NUMBER</string>
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
  <!-- pumi:// links open in the app. -->
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

IDENTITY="${SIGN_IDENTITY:--}"
KEYCHAIN_FLAGS=()
[[ -n "${SIGN_KEYCHAIN:-}" ]] && KEYCHAIN_FLAGS=(--keychain "$SIGN_KEYCHAIN")
SIGN_FLAGS=(--timestamp=none)
ENTITLEMENTS=()
HOST_ENTITLEMENTS=()
if [[ "$IDENTITY" == Developer\ ID* ]]; then
  # The hardened runtime is what notarization requires. The app compiles code as it runs (it is
  # Bun): `allow-jit` is what JSC needs for that, and without it the engine loses
  # `SharedArrayBuffer`, which the worker the windows run in is built on — the app would not
  # start.
  # pumi-host loads packages' native libraries, which the hardened runtime's library
  # validation would refuse.
  APP_ENT="$STAGE/pumi-entitlements.plist"
  cat > "$APP_ENT" <<'PLIST'
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
  HOST_ENT="$STAGE/pumi-host-entitlements.plist"
  cat > "$HOST_ENT" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>com.apple.security.cs.disable-library-validation</key>
  <true/>
</dict>
</plist>
PLIST
  SIGN_FLAGS=(--options runtime --timestamp)
  ENTITLEMENTS=(--entitlements "$APP_ENT")
  HOST_ENTITLEMENTS=(--entitlements "$HOST_ENT")
fi

# Extended attributes (provenance, Finder info) are not the app: codesign refuses some, and a
# zip would carry each as a `._*` file.
xattr -cr "$BUNDLE" 2>/dev/null || true
say "signing (${IDENTITY/#-/ad-hoc})"
# Inside out: every executable in the bundle carries its own signature, then the bundle seals
# them. Each gets an identifier under the bundle's, so their requirements are as stable.
# `${a[@]+"${a[@]}"}` because macOS's bash 3.2 calls an empty array unbound under `set -u`.
sign() { codesign --force "${SIGN_FLAGS[@]}" ${KEYCHAIN_FLAGS[@]+"${KEYCHAIN_FLAGS[@]}"} --sign "$IDENTITY" "$@"; }
sign -i "$BUNDLE_ID.addon" "$BUNDLE/Contents/MacOS/$ADDON"
sign -i "$BUNDLE_ID.pumid" "$BUNDLE/Contents/MacOS/pumid"
sign ${HOST_ENTITLEMENTS[@]+"${HOST_ENTITLEMENTS[@]}"} -i "$BUNDLE_ID.host" "$BUNDLE/Contents/MacOS/pumi-host"
sign -i "$BUNDLE_ID.cli" "$BUNDLE/Contents/Resources/bin/pumi-cli"
sign ${ENTITLEMENTS[@]+"${ENTITLEMENTS[@]}"} -i "$BUNDLE_ID" "$BUNDLE"
codesign --verify --deep --strict "$BUNDLE" || die "the bundle's signature does not verify"
codesign -d -r- "$BUNDLE" 2>&1 | sed -n 's/^designated => /    requirement: /p'

if [[ -z "${NO_ZIP:-}" ]]; then
  # ditto keeps the signature and symlinks exactly; install.sh and the updater unpack it
  # with ditto too.
  zip_bundle "$BUNDLE" "$ZIP"
  write_sha256 "$ZIP"
  zip_symbols "$BUILT" "$OUT/$APP_NAME-$VERSION-macos-$ARCH-symbols.zip"
  say "packed $ZIP ($(( $(file_size "$ZIP") / 1024 / 1024 )) MB)"
fi
