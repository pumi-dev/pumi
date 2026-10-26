#!/usr/bin/env bash
#
# Assemble and pack pumi for Windows x86_64, from what `build.sh x86_64-pc-windows-msvc` built.
#
#   scripts/release/pack-windows.sh      # -> $OUT/pumi-<version>-windows-x86_64.zip
#
# The zip is one self-contained directory, `pumi.app\`, laid out like the macOS bundle so the
# app finds its pieces the same way (`<exe>\..\Resources\...`):
#
#   pumi.app\bin\pumi.exe               the app
#   pumi.app\bin\pumid.exe, pumi-host.exe   the terminal server and its host, started by the app
#   pumi.app\bin\pumi.win32-x64.node    the addon, beside the app that dlopens it
#   pumi.app\Resources\bin\             pumi-cli.exe, and `pumi.exe` as a copy of it
#   pumi.app\Resources\runtime\         the bundled plugins, the files `pumi do` runs
#   pumi.app\Resources\assets\          the icons every theme draws
#   pumi.app\Resources\NOTICE*          the third-party notices
#   pumi.app\VERSION                    what the updater compares against
#
# install.ps1 puts it at %LOCALAPPDATA%\Programs\pumi\pumi.app; the updater replaces that
# directory whole. There is no icon or desktop entry: Windows takes both from the shortcut the
# installer makes, and v1 ships no .ico.

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

TRIPLE="x86_64-pc-windows-msvc"
BUILT="${BUILT_DIR:-$TARGET_DIR/$TRIPLE/release}"
[[ -f "$BUILT/pumi.exe" ]] || die "no build of pumi for $TRIPLE (run scripts/release/build.sh $TRIPLE)"

STAGE="$OUT/windows-x86_64"
ROOT="$STAGE/$APP_NAME.app"
ZIP="$OUT/$APP_NAME-$VERSION-windows-x86_64.zip"

say "assembling $ROOT"
rm -rf "$ROOT"
mkdir -p "$ROOT/bin" "$ROOT/Resources"

for bin in pumi.exe "$(addon_name "$TRIPLE")" pumid.exe pumi-host.exe; do
  cp "$BUILT/$bin" "$ROOT/bin/$bin"
done
copy_cli "$BUILT" "$ROOT/Resources" .exe
copy_runtime "$ROOT/Resources"
copy_assets "$ROOT/Resources"
copy_notices "$ROOT/Resources"

printf '%s\n' "$VERSION" > "$ROOT/VERSION"

zip_plain "$ROOT" "$ZIP"
write_sha256 "$ZIP"
say "packed $ZIP ($(( $(file_size "$ZIP") / 1024 / 1024 )) MB)"
