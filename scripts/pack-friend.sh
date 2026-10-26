#!/usr/bin/env bash
#
# A build for one friend: `pumi.app` with the plugins this machine has turned on, and an
# `install.sh` beside it, as one zip to send.
#
#   scripts/pack-friend.sh                        # -> dist/pumi-<version>-friend-arm64.zip
#   PLUGINS="git files win98" scripts/pack-friend.sh
#   PLUGIN_LOCK=~/.config/pumi-dev/plugin-lock.json scripts/pack-friend.sh
#
# What ships as a plugin is a folder in the app at `Resources/runtime/bundled/<name>`
# (`crates/pumi-host/src/plugins.rs`): pumi lists it like any other plugin and can turn it off,
# but never updates or removes it — so a copy that is installed is the copy that was built.
#
# The app is built here, from this checkout, exactly as `scripts/install.sh` builds it for this
# Mac; nothing is installed. The zip unpacks to `pumi/` holding `pumi.app`, `install.sh` and
# `README.txt`.
source "$(dirname "${BASH_SOURCE[0]}")/release/common.sh"

# What a plugin imports that the app does not serve itself. The app answers `react` and `pumi`
# wherever they run (`runtime/app/app/ui.ts`, `runtime/app/worker/worker.ts`); anything else is
# the plugin's own, and comes with it from its `node_modules`.
DEPENDS="heeler:qrcode"

say "packing pumi for a friend (this Mac: $(uname -m))"
STAGE="$(mktemp -d)"
trap 'rm -rf "$STAGE"' EXIT

# ── What goes in ─────────────────────────────────────────────────────
# The plugins: the ones this machine's lockfile says are on, or the ones PLUGINS names — a folder
# each, or a name from the lockfile. A plugin whose folder is gone (a checkout moved) is left
# out, and said so.
LOCK="${PLUGIN_LOCK:-$HOME/.config/pumi/plugin-lock.json}"
LIST="$STAGE/plugins"
if [[ -n "${PLUGINS:-}" ]]; then
  for word in $PLUGINS; do
    if [[ -f "$word/package.json" ]]; then
      printf '%s\t%s\n' "$(basename "${word%/}")" "$(cd "$word" && pwd)" >> "$LIST"
    else
      src="$(python3 - "$LOCK" "$word" <<'PY'
import json, sys
try:
    entry = json.load(open(sys.argv[1]))["plugins"].get(sys.argv[2], {})
except FileNotFoundError:
    entry = {}
print(entry.get("src", ""))
PY
)"
      [[ -n "$src" ]] || die "PLUGINS names `$word`, which is not a folder nor in $LOCK"
      printf '%s\t%s\n' "$word" "$src" >> "$LIST"
    fi
  done
else
  [[ -f "$LOCK" ]] || die "no plugin lockfile at $LOCK (name them: PLUGINS=\"git files\")"
  python3 - "$LOCK" >> "$LIST" <<'PY'
import json, sys
plugins = json.load(open(sys.argv[1]))["plugins"]
for name, entry in sorted(plugins.items()):
    if entry.get("disabled"):
        continue
    src = entry.get("src")
    if src and entry.get("kind") == "link":
        print(f"{name}\t{src}")
PY
fi
[[ -s "$LIST" ]] || die "no plugins to ship: the app would have pumi's own and nothing else"

# ── The build ────────────────────────────────────────────────────────
# The app is a Bun process: `bun run app` compiles it to target/pumi, with its addon (the root
# crate) beside it, `target/pumi.<platform>-<arch>.node`, which it dlopens from there.
# The app is built last: it is the quick one, and the check below then has the least between the
# sources being read and the binary being written.
say "building the server and the shims (release)"
( cd "$REPO" && cargo build --release -p pumid -p pumi-host -p pumi-cli )
say "building the app and its addon"
( cd "$REPO/runtime/app" && bun run addon --release && bun run app )

# What is packed has to be this checkout, compiled after the last edit.
say "checking the build is of this checkout"
STALE="$(find "$REPO/runtime/app/app" "$REPO/runtime/app/worker" "$REPO/runtime/app/sdk" \
  "$REPO/runtime/app/react" "$REPO/runtime/app/build" -type f -newer "$REPO/target/pumi" 2>/dev/null | head -3)"
[[ -n "$STALE" ]] && die "target/pumi is older than its own sources ($(echo $STALE)): the app did not rebuild"
for binary in pumid pumi-host pumi-cli; do
  STALE="$(find "$REPO/crates/$binary/src" -name '*.rs' -newer "$REPO/target/release/$binary" 2>/dev/null | head -1)"
  [[ -n "$STALE" ]] && die "target/release/$binary is older than $STALE: it did not rebuild"
done

# ── The bundle ───────────────────────────────────────────────────────
# `pack-macos.sh` is the release's own assembler: the binaries, the runtime, the assets, the icon,
# Info.plist, the signature (ad-hoc here). It reads them from one directory, which for a release
# is cargo's per-target one and for this build is `target/` plus `target/release/`.
BUILT="$STAGE/built"
mkdir -p "$BUILT"
ADDON="$(addon_name aarch64-apple-darwin)"
cp "$REPO/target/pumi" "$REPO/target/$ADDON" "$BUILT/"
cp "$REPO/target/release/pumid" "$REPO/target/release/pumi-host" "$REPO/target/release/pumi-cli" "$BUILT/"

BUNDLE_ID="${BUNDLE_ID:-local.pumi}"
BUILT_DIR="$BUILT" BUNDLE_ID="$BUNDLE_ID" OUT="$STAGE/pack" NO_ZIP=1 \
  "$REPO/scripts/release/pack-macos.sh" aarch64
BUNDLE="$STAGE/pack/macos-aarch64/$APP_NAME.app"
[[ -d "$BUNDLE" ]] || die "pack-macos.sh made no bundle"
RUNTIME_PLUGINS="$BUNDLE/Contents/Resources/runtime/bundled"

# ── The plugins ──────────────────────────────────────────────────────
SHIPPED=()
while IFS=$'\t' read -r name src; do
  [[ -n "$name" ]] || continue
  if [[ -e "$RUNTIME_PLUGINS/$name" ]]; then
    say "leaving out $name: pumi already bundles a plugin by that name"
    continue
  fi
  if [[ ! -f "$src/package.json" ]]; then
    say "leaving out $name: no package.json in $src (its folder is gone)"
    continue
  fi
  say "plugin $name, from $src"
  DEST="$RUNTIME_PLUGINS/$name"
  mkdir -p "$DEST"
  # Its own files, without build output or a test suite.
  ( cd "$src" && tar -cf - --exclude node_modules --exclude test --exclude target --exclude dist \
      --exclude .DS_Store . ) | ( cd "$DEST" && tar -xf - )
  [[ -f "$DEST/package.json" ]] || die "$name did not copy whole"
  # Whatever it imports that the app does not answer, from its own node_modules.
  for entry in $DEPENDS; do
    [[ "${entry%%:*}" == "$name" ]] || continue
    for package in ${entry#*:}; do
      [[ -d "$src/node_modules/$package" ]] || die "$name needs \`$package\` but has no node_modules/$package"
      mkdir -p "$DEST/node_modules"
      # -L: the packages are symlinks into the workspace's store; what ships is the code.
      cp -RL "$src/node_modules/$package" "$DEST/node_modules/$package"
      say "  carrying $package ($(du -sh "$DEST/node_modules/$package" | cut -f1))"
    done
  done
  SHIPPED+=("$name")
done < "$LIST"

[[ ${#SHIPPED[@]} -gt 0 ]] || die "no plugins to ship: the app would have pumi's own and nothing else"

# ── The seal ─────────────────────────────────────────────────────────
# The bundle was signed before the plugins went in, and a signature seals what is inside it: the
# app would be a broken bundle with them there. Signed again, ad-hoc, as pack-macos.sh does for a
# build without a certificate — no hardened runtime, so the addon and the JIT need nothing extra.
say "signing the bundle again, with the plugins in it"
xattr -cr "$BUNDLE" 2>/dev/null || true
codesign --force --sign - --identifier "$BUNDLE_ID" "$BUNDLE" \
  || die "codesign failed: the app would not launch"
codesign --verify --deep --strict "$BUNDLE" || die "the bundle's signature does not verify"

# ── The zip ──────────────────────────────────────────────────────────
FOLDER="$STAGE/$APP_NAME"
mkdir -p "$FOLDER"
# ditto, not cp: the signature and the symlinks have to come along exactly, as `install.sh` and
# the app's updater unpack them.
ditto "$BUNDLE" "$FOLDER/$APP_NAME.app"
sed "s/@VERSION@/$VERSION/" "$REPO/scripts/friend/install.sh" > "$FOLDER/install.sh"
chmod 755 "$FOLDER/install.sh"

cat > "$FOLDER/README.txt" <<README
pumi $VERSION — a build to try
$(printf -- '─%.0s' $(seq 1 40))

How to install (2 minutes, nothing else needed):

  1. Unzip this folder (double-click the .zip, if it is still zipped).
  2. Open Terminal, and drag the folder into it after "cd ":
        cd <the folder you just unzipped>
  3. Run:  sh install.sh

That copies pumi.app into /Applications, clears the "downloaded from the internet" flag that
would make macOS complain, and opens the app. Nothing else is installed: the terminal server and
every plugin are inside the app.

Plugins already on, in this build ($((${#SHIPPED[@]}))):
$(printf '  %s\n' "${SHIPPED[@]}")

They are the plugins from this machine, built into the app. Settings > Plugins turns any of them
off; that is all it can do to them, they come with the app.

Notes

  * It is a test build, not a release: updates are off (Settings > Updates), so it will not
    replace itself with the official version.
  * If macOS ever says it "cannot be opened because the developer cannot be verified", open
     System Settings > Privacy & Security and click "Open Anyway" — or run
        xattr -dr com.apple.quarantine /Applications/pumi.app
  * To uninstall: drag pumi.app to the Bin. Settings and plugin data stay in ~/.config/pumi
     and ~/.local/share/pumi.
README

mkdir -p "$REPO/dist"
ZIP="$REPO/dist/$APP_NAME-$VERSION-friend-$(uname -m).zip"
zip_bundle "$FOLDER" "$ZIP"
write_sha256 "$ZIP"

say "packed $ZIP ($(( $(file_size "$ZIP") / 1024 / 1024 )) MB)"
echo "    plugins:   ${#SHIPPED[@]} — ${SHIPPED[*]}"
echo "    send it:   $(basename "$ZIP") and its .sha256"
