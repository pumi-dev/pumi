#!/usr/bin/env bash
#
# Build everything a release carries, for one or more targets: the app (a Bun executable) and
# its addon, pumid, pumi-host and the CLI, into cargo's per-target dir, where the pack scripts
# find them, with the .dSYMs beside them.
#
#   scripts/release/build.sh                                  # this machine
#   scripts/release/build.sh aarch64-apple-darwin x86_64-apple-darwin
#   scripts/release/build.sh x86_64-pc-windows-msvc           # cargo xwin, off Windows
#
# Needs Bun, and Zig 0.16 (`ZIG`, else .cargo/config.toml's) for libghostty-vt. A Windows target
# built on another OS also needs `cargo install cargo-xwin` (napi installs it for the addon);
# a Bun executable for another OS is Bun's own cross-compile, which downloads that OS's runtime.

source "$(dirname "${BASH_SOURCE[0]}")/common.sh"

# What the updater is built to trust and where it looks (`src/updater/fetch.rs`). Release
# builds turn it on; a plain `cargo build` has it off, so a dev build never replaces itself.
export PUMI_UPDATER=1
export PUMI_RELEASE_CHANNEL="$CHANNEL"
export PUMI_RELEASE_REPO="${PUMI_RELEASE_REPO:-pumi-dev/pumi}"

triples=("$@")
[[ ${#triples[@]} -gt 0 ]] || triples=("$(host_target)")

# The cargo subcommand for a target: cargo-xwin links MSVC binaries on a host that is not
# Windows, with LLVM and Microsoft's libraries.
cargo_subcommand() {
  case "$1" in
    *-pc-windows-msvc)
      case "$(uname -s)" in
        MINGW*|MSYS*|CYGWIN*) ;;
        *) echo xwin ;;
      esac
      ;;
  esac
}

cd "$REPO"
(cd runtime/app && bun install --frozen-lockfile)

for triple in "${triples[@]}"; do
  BUILT="$TARGET_DIR/$triple/release"
  EXE=""
  [[ "$triple" == *-pc-windows-msvc ]] && EXE=.exe

  say "building pumid, pumi-host and the CLI for $triple"
  subcommand="$(cargo_subcommand "$triple")"
  if [[ -z "$subcommand" ]]; then
    cargo build --release --locked --target "$triple" -p pumid -p pumi-host -p pumi-cli
  else
    command -v "cargo-$subcommand" >/dev/null 2>&1 || die "cargo $subcommand is missing for $triple: cargo install cargo-$subcommand"
    cargo "$subcommand" build --release --locked --target "$triple" -p pumid -p pumi-host -p pumi-cli
  fi

  # The app and the addon last: `target/pumi` is one file for every target, so the pack scripts
  # read each target's from cargo's dir, where this puts it.
  say "building the addon and the app for $triple"
  (cd runtime/app && bun run addon --release --target "$triple" && bun run app --target "$triple")
  cp "$REPO/target/pumi$EXE" "$BUILT/pumi$EXE"
  cp "$REPO/runtime/app/native/$(addon_name "$triple")" "$BUILT/$(addon_name "$triple")"

  case "$triple" in
    *-apple-darwin)
      for dsym in "$BUILT/libpumi.dylib.dSYM" "$TARGET_DIR/release/libpumi.dylib.dSYM"; do
        [[ -d "$dsym" ]] || continue
        rm -rf "$BUILT/$(addon_name "$triple").dSYM"
        cp -RL "$dsym" "$BUILT/$(addon_name "$triple").dSYM"
        break
      done
      ;;
  esac

  say "built in $BUILT"
done
