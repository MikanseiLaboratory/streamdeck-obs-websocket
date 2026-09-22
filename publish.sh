#!/usr/bin/env bash
set -euo pipefail
ROOT="$(cd "$(dirname "$0")" && pwd)"
PLUGIN_ID="dev.flowingspdg.multiobs.rust"
PLUGIN_DIR="$ROOT/$PLUGIN_ID.sdPlugin"
OUTPUT_DIR="${OUTPUT_DIR:-$ROOT/artifacts/plugin}"
INSTALL="${INSTALL:-0}"
PACK="${PACK:-0}"

publish_target() {
  local target="$1"
  local rid="$2"
  local name="$3"
  rustup target add "$target"
  cargo build -p multiobs-plugin --release --bin multiobs_plugin --target "$target"
  mkdir -p "$PLUGIN_DIR/bin/$rid"
  local src="$ROOT/target/$target/release/multiobs_plugin"
  if [[ "$name" == *.exe ]]; then
    src="${src}.exe"
  fi
  cp "$src" "$PLUGIN_DIR/bin/$rid/$name"
}

cd "$ROOT"
cargo run -p multiobs-plugin --bin typegen
(cd "$ROOT/pi" && { [ -d node_modules ] || npm install; } && npm run build)

publish_target x86_64-pc-windows-msvc win-x64 "$PLUGIN_ID.exe" || true
publish_target aarch64-apple-darwin osx-arm64 "$PLUGIN_ID" || true
publish_target x86_64-apple-darwin osx-x64 "$PLUGIN_ID" || true

mkdir -p "$PLUGIN_DIR/bin/osx"
ARM="$PLUGIN_DIR/bin/osx-arm64/$PLUGIN_ID"
X64="$PLUGIN_DIR/bin/osx-x64/$PLUGIN_ID"
UNIVERSAL="$PLUGIN_DIR/bin/osx/$PLUGIN_ID"
if [[ -f "$ARM" && -f "$X64" ]]; then
  lipo -create "$ARM" "$X64" -output "$UNIVERSAL"
elif [[ -f "$ARM" ]]; then
  cp "$ARM" "$UNIVERSAL"
elif [[ -f "$X64" ]]; then
  cp "$X64" "$UNIVERSAL"
fi

if [[ "$PACK" == "1" ]]; then
  mkdir -p "$OUTPUT_DIR"
  ZIP="$OUTPUT_DIR/$PLUGIN_ID.streamDeckPlugin"
  rm -f "$ZIP"
  if command -v streamdeck >/dev/null 2>&1; then
    streamdeck pack "$PLUGIN_DIR" --output "$OUTPUT_DIR" --force
  else
    (cd "$(dirname "$PLUGIN_DIR")" && zip -qr "$ZIP" "$(basename "$PLUGIN_DIR")")
  fi
  echo "Packed $ZIP"
fi

if [[ "$INSTALL" == "1" ]]; then
  DEST="$HOME/Library/Application Support/com.elgato.StreamDeck/Plugins/$PLUGIN_ID.sdPlugin"
  mkdir -p "$(dirname "$DEST")"
  rm -rf "$DEST"
  cp -R "$PLUGIN_DIR" "$DEST"
  echo "Installed plugin to $DEST"
  if command -v streamdeck >/dev/null 2>&1; then
    streamdeck restart "$PLUGIN_ID"
  fi
fi
