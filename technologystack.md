# 技術スタック

変更する場合は承認を得ること。

- Rust 1.85 (edition 2021)
- `streamdeck-plugin` 0.1.0 (`macros`, `typegen`)
- `obs-websocket` 0.1.0（git rev `8fe8125ffa23e67c76741c3bfdb74b52a3a7e907`、`default-features = false`）
- `obs-websocket-core` 0.1.0（同じ rev）
- `tokio` 1
- `tokio-tungstenite` 0.26（テスト用モック専用）
- Property Inspector: React 18、Vite 6、TypeScript 5、`@mikanseilaboratory/streamdeck-pi-client` 0.1.0
- 設定型の共有: `ts-rs` 10.1
- 対象: Windows x64、macOS arm64、macOS x64
- Stream Deck SDK 2、OBS WebSocket v5 (OBS Studio 30.2 以降)
