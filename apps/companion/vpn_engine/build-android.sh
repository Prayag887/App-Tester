#!/usr/bin/env bash
set -euo pipefail

engine_root="$(cd "$(dirname "$0")" && pwd)"
app_root="$(cd "$engine_root/.." && pwd)"
ndk_root="${ANDROID_NDK_HOME:-$HOME/Library/Android/sdk/ndk/28.2.13676358}"
prebuilt_root="$ndk_root/toolchains/llvm/prebuilt"
toolchain="$(find "$prebuilt_root" -maxdepth 1 -mindepth 1 -type d -name 'darwin-*' | head -n 1)/bin"

if [[ ! -x "$toolchain/aarch64-linux-android26-clang" ]]; then
  echo "A compatible Android NDK is required. Set ANDROID_NDK_HOME to its root." >&2
  exit 1
fi

build_abi() {
  local abi="$1" goarch="$2" compiler="$3"
  local output="$app_root/android/app/src/main/jniLibs/$abi"
  mkdir -p "$output"
  CGO_ENABLED=1 GOOS=android GOARCH="$goarch" CC="$toolchain/$compiler" \
    go build -buildmode=c-shared -trimpath -ldflags='-s -w' \
    -o "$output/libapptester_tun2socks.so" .
  rm -f "$output/libapptester_tun2socks.h"
}

cd "$engine_root"
build_abi arm64-v8a arm64 aarch64-linux-android26-clang
build_abi x86_64 amd64 x86_64-linux-android26-clang
