#!/usr/bin/env bash
#
# Write the channel's manifest for the artifacts in $OUT, then sign everything there.
#
#   MINISIGN_KEY=path/to/secret.key scripts/release/manifest.sh
#
# latest.json is what install.sh and the app's updater read first. Its signature is the
# root of trust: it names every artifact's SHA-256, so an artifact is trusted by matching
# the signed manifest, wherever it was downloaded from. The `.minisig` beside each artifact
# is for people verifying a download by hand (`minisign -Vm <file> -P <key>`).
#
#   {
#     "schema": 1, "name": "pumi", "channel": "preview",
#     "version": "0.42.0", "build": 1, "tag": "v0.42.0", "pub_date": "2026-…Z",
#     "notes_url": "https://github.com/<repo>/releases/tag/v0.42.0",
#     "assets": {
#       "macos-aarch64": { "name": "pumi-0.42.0-macos-aarch64.zip", "sha256": "…",
#                          "size": 123, "url": "https://github.com/<repo>/releases/download/v0.42.0/…" },
#       "linux-x86_64": { "name": "pumi-0.42.0-linux-x86_64.tar.gz", … },
#       "windows-x86_64": { "name": "pumi-0.42.0-windows-x86_64.zip", … },
#     }
#   }
#
# `url` needs the release to be public; the updater and install.sh also find the asset by
# `name` through GitHub's API, which is what they fall back to.
#
# Without MINISIGN_KEY the manifest is written and nothing is signed (a dry run).

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

RELEASE_REPO="${PUMI_RELEASE_REPO:-pumi-dev/pumi}"
TAG="v$VERSION"
# Absolute: the script works from inside $OUT, and the workflow passes a relative one.
OUT="$(cd "$OUT" && pwd)"
MANIFEST="$OUT/latest.json"

cd "$OUT"
assets=()
# One entry per artifact: `macos-aarch64.zip`, `linux-x86_64.tar.gz`, `windows-x86_64.zip` —
# the platform is the name between the version and the extension (`src/updater/manifest.rs`).
for file in "$APP_NAME-$VERSION"-macos-*.zip "$APP_NAME-$VERSION"-linux-*.tar.gz "$APP_NAME-$VERSION"-windows-*.zip; do
  [[ -f "$file" && "$file" != *-symbols.zip ]] || continue
  platform="${file#"$APP_NAME-$VERSION-"}"
  platform="${platform%.zip}"
  platform="${platform%.tar.gz}"
  assets+=("$platform|$file|$(sha256_of "$file")|$(file_size "$file")")
done
[[ ${#assets[@]} -gt 0 ]] || die "no artifacts for $VERSION in $OUT"

say "writing $MANIFEST (${#assets[@]} platforms)"
python3 - "$MANIFEST" "$VERSION" "$BUILD_NUMBER" "$CHANNEL" "$RELEASE_REPO" "$TAG" "${assets[@]}" <<'PY'
import json, sys, datetime
out, version, build, channel, repo, tag, *assets = sys.argv[1:]
manifest = {
    "schema": 1,
    "name": "pumi",
    "channel": channel,
    "version": version,
    "build": int(build),
    "tag": tag,
    "pub_date": datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
    "notes_url": f"https://github.com/{repo}/releases/tag/{tag}",
    "assets": {},
}
for entry in assets:
    platform, name, sha, size = entry.split("|")
    manifest["assets"][platform] = {
        "name": name,
        "sha256": sha,
        "size": int(size),
        "url": f"https://github.com/{repo}/releases/download/{tag}/{name}",
    }
with open(out, "w") as f:
    json.dump(manifest, f, indent=2)
    f.write("\n")
PY

if [[ -z "${MINISIGN_KEY:-}" ]]; then
  say "MINISIGN_KEY unset: nothing signed (dry run)"
  exit 0
fi
command -v minisign >/dev/null 2>&1 || die "minisign is not installed"

say "signing the manifest and artifacts"
for file in latest.json "$APP_NAME-$VERSION"-*.zip "$APP_NAME-$VERSION"-*.tar.gz; do
  [[ -f "$file" ]] || continue
  # Symbols are kept beside the artifact for reading crashes, not installed from: nothing about
  # them is trusted, and a `.sha256`/`.minisig` is never signed by its own name.
  case "$file" in *-symbols.zip|*.sha256|*.minisig) continue ;; esac
  # The trusted comment is signed too: it pins the file's name and version, so a valid
  # signature cannot be replayed onto another file.
  minisign -S -s "$MINISIGN_KEY" -m "$file" -x "$file.minisig" \
    -t "pumi $VERSION $CHANNEL file:$file" </dev/null >/dev/null
  echo "    signed $file"
done
