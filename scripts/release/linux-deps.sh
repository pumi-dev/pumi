#!/usr/bin/env bash
#
# What a Linux machine needs to build pumi: the system libraries gpui links (X11, Wayland,
# xkbcommon, fontconfig, Vulkan's loader), GTK and WebKitGTK for web pages (wry), a C toolchain,
# Zig 0.16 for libghostty-vt and the remote servers, ImageMagick to cut the icons pack-linux.sh
# ships, and Rust with the targets a release builds.
#
#   sudo -E scripts/release/linux-deps.sh        # Debian/Ubuntu (CI runs ubuntu-22.04)
#
# Zig goes to /opt/zig-0.16.0 and its path is printed (export ZIG=… for cargo).

set -euo pipefail

ZIG_VERSION=0.16.0
case "$(uname -m)" in
  x86_64) ZIG_ARCH=x86_64; ZIG_SHA=70e49664a74374b48b51e6f3fdfbf437f6395d42509050588bd49abe52ba3d00 ;;
  aarch64) ZIG_ARCH=aarch64; ZIG_SHA=ea4b09bfb22ec6f6c6ceac57ab63efb6b46e17ab08d21f69f3a48b38e1534f17 ;;
  *) echo "unsupported architecture $(uname -m)" >&2; exit 1 ;;
esac

export DEBIAN_FRONTEND=noninteractive
apt-get update -qq
apt-get install -y -qq --no-install-recommends \
  build-essential clang cmake pkg-config git curl ca-certificates xz-utils file \
  libasound2-dev libfontconfig-dev libfreetype-dev libwayland-dev libx11-dev libx11-xcb-dev \
  libxcb1-dev libxkbcommon-dev libxkbcommon-x11-dev libvulkan-dev libssl-dev libzstd-dev \
  zlib1g-dev python3 imagemagick \
  libgtk-3-dev libwebkit2gtk-4.1-dev libsoup-3.0-dev libjavascriptcoregtk-4.1-dev

if [[ ! -x "/opt/zig-$ZIG_VERSION/zig" ]]; then
  tarball="zig-$ZIG_ARCH-linux-$ZIG_VERSION.tar.xz"
  curl -fsSL -o "/tmp/$tarball" "https://ziglang.org/download/$ZIG_VERSION/$tarball"
  echo "$ZIG_SHA  /tmp/$tarball" | sha256sum -c -
  mkdir -p "/opt/zig-$ZIG_VERSION"
  tar -xJf "/tmp/$tarball" -C "/opt/zig-$ZIG_VERSION" --strip-components=1
  rm "/tmp/$tarball"
fi
echo "ZIG=/opt/zig-$ZIG_VERSION/zig"
