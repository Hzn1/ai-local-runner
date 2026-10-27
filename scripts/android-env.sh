#!/usr/bin/env bash
# Source this file. Existing SDK/JDK installations take precedence.
TNN_ANDROID_TOOLS="${TNN_ANDROID_TOOLS:-$HOME/.local/share/tnn-android-tools}"
export ANDROID_HOME="${ANDROID_HOME:-$TNN_ANDROID_TOOLS/sdk}"
export NDK_HOME="${NDK_HOME:-$ANDROID_HOME/ndk/28.2.13676358}"
if [[ -z "${JAVA_HOME:-}" ]]; then
  for tnn_jdk in "$TNN_ANDROID_TOOLS"/jdk-21*; do
    if [[ -x "$tnn_jdk/bin/java" ]]; then export JAVA_HOME="$tnn_jdk"; break; fi
  done
fi
export PATH="${JAVA_HOME:+$JAVA_HOME/bin:}$ANDROID_HOME/platform-tools:$ANDROID_HOME/cmake/3.22.1/bin:$PATH"
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-2}"
export CARGO_INCREMENTAL=0
export CARGO_PROFILE_DEV_DEBUG=0
export CARGO_PROFILE_TEST_DEBUG=0
# Mobile cross builds need the NDK compiler for bundled SQLite/ring too.
tnn_ndk_bin="$NDK_HOME/toolchains/llvm/prebuilt/linux-x86_64/bin"
export CC_aarch64_linux_android="$tnn_ndk_bin/aarch64-linux-android28-clang"
export CXX_aarch64_linux_android="$tnn_ndk_bin/aarch64-linux-android28-clang++"
export AR_aarch64_linux_android="$tnn_ndk_bin/llvm-ar"
export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$CC_aarch64_linux_android"
# NDK r28's linker already defaults to 16 KB alignment. Let Tauri supply its
# Android system-library flags consistently to both CLI and Gradle builds.
unset tnn_jdk tnn_ndk_bin
