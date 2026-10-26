#!/usr/bin/env bash
#
# Assemble and pack pumi for one Linux architecture, from what build.sh built.
#
#   scripts/release/pack-linux.sh x86_64      # -> $OUT/pumi-<version>-linux-x86_64.tar.gz
#
# The tarball is one self-contained directory, `pumi.app/`, laid out so the app finds its
# pieces exactly as it does in the macOS bundle (`<exe>/../Resources/...`):
#
#   pumi.app/bin/pumi                 the app
#   pumi.app/bin/pumid, pumi-host     the terminal server and its host, started by the app
#   pumi.app/bin/pumi.linux-*.node    the addon, beside the app that dlopens it
#   pumi.app/Resources/bin/           pumi-cli, and `pumi` linking to it
#   pumi.app/Resources/runtime/       the bundled plugins, the themes, the files `pumi do` runs
#   pumi.app/Resources/assets/        the icons every theme draws
#   pumi.app/Resources/NOTICE*        the third-party notices
#   pumi.app/share/applications/      the .desktop entry (install.sh fills in its paths)
#   pumi.app/share/icons/hicolor/     the icon
#   pumi.app/VERSION                  what the updater compares against
#
# install.sh puts it at ~/.local/pumi.app; the updater replaces that directory whole.

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

ARCH="${1:?usage: pack-linux.sh <x86_64|aarch64>}"
TRIPLE="$ARCH-unknown-linux-gnu"
APP_ID="dev.pumi.app"
BUILT="${BUILT_DIR:-$TARGET_DIR/$TRIPLE/release}"
[[ -x "$BUILT/pumi" ]] || die "no build of pumi for $TRIPLE (run scripts/release/build.sh $TRIPLE)"

STAGE="$OUT/linux-$ARCH"
ROOT="$STAGE/$APP_NAME.app"
TARBALL="$OUT/$APP_NAME-$VERSION-linux-$ARCH.tar.gz"

say "assembling $ROOT"
rm -rf "$ROOT"
mkdir -p "$ROOT/bin" "$ROOT/Resources" "$ROOT/share/applications"

for bin in pumi "$(addon_name "$TRIPLE")" pumid pumi-host; do
  install -m 755 "$BUILT/$bin" "$ROOT/bin/$bin"
done
copy_cli "$BUILT" "$ROOT/Resources"
copy_runtime "$ROOT/Resources"
copy_assets "$ROOT/Resources"
copy_notices "$ROOT/Resources"

# The hicolor icons are cut from the one master at pack time (scripts/make-icon.sh does the
# same for the site's). ImageMagick 7 is `magick`; the 6 that Ubuntu 22.04 ships is `convert`.
ICON_SOURCE="$REPO/assets/brand/app-icon.png"
[[ -f "$ICON_SOURCE" ]] || die "no app icon at $ICON_SOURCE"
MAGICK="$(command -v magick || command -v convert || true)"
[[ -n "$MAGICK" ]] || die "pack-linux.sh needs ImageMagick (magick) to cut the icons"
for size in 128 256 512; do
  mkdir -p "$ROOT/share/icons/hicolor/${size}x${size}/apps"
  "$MAGICK" "$ICON_SOURCE" -filter Lanczos -resize "${size}x${size}" -depth 8 -strip \
    -define png:compression-level=9 -type TrueColorAlpha \
    "$ROOT/share/icons/hicolor/${size}x${size}/apps/$APP_ID.png"
done

# `@EXEC@` is filled in by install.sh with where the app actually is.
cat > "$ROOT/share/applications/$APP_ID.desktop" <<DESKTOP
[Desktop Entry]
Type=Application
Name=pumi
GenericName=Terminal Workspace
Comment=Terminals and coding agents side by side
Exec=@EXEC@ %U
Icon=$APP_ID
Terminal=false
Categories=Development;System;TerminalEmulator;
Keywords=terminal;agent;shell;
StartupWMClass=$APP_ID
MimeType=x-scheme-handler/pumi;
DESKTOP

printf '%s\n' "$VERSION" > "$ROOT/VERSION"

rm -f "$TARBALL"
tar -C "$STAGE" -czf "$TARBALL" "$APP_NAME.app"
write_sha256 "$TARBALL"
say "packed $TARBALL ($(( $(file_size "$TARBALL") / 1024 / 1024 )) MB)"
