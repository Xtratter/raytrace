#!/data/data/com.termux/files/usr/bin/bash
# Builds libraytrace.so. Termux: native build. CI/PC: TARGET=aarch64-linux-android (+ linker env, see workflow).
set -euo pipefail
cd "$(dirname "$0")/../rust"
args=(build --release --lib)
out=target/release
if [ -n "${TARGET:-}" ]; then args+=(--target "$TARGET"); out="target/$TARGET/release"; fi
for i in 1 2 3; do cargo "${args[@]}" && break || { echo "cargo failed (try $i)"; [ "$i" = 3 ] && exit 1; sleep 2; }; done
mkdir -p ../app/src/main/jniLibs/arm64-v8a
cp "$out/libraytrace.so" ../app/src/main/jniLibs/arm64-v8a/
