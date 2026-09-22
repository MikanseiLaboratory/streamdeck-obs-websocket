# Multi OBS

任意の台数の OBS Studio を、名前付きインスタンスとして Stream Deck から操作するプラグインです。接続先は固定の 2 台ではなく、追加、削除、並べ替えができます。アクションは「すべての有効なインスタンス」「グループ」「個別選択」のいずれかにファンアウトします。

## 必要環境

- Stream Deck 6.4 以降
- OBS Studio 30.2 以降 (obs-websocket 5.5。OBS に同梱されています)
- 各 OBS の WebSocket サーバを有効にし、ポートが重ならないようにする

## ビルド

```bash
cargo test --workspace
./publish.sh
```

macOS では arm64 と x64 をユニバーサルバイナリ `bin/plugin` にまとめます。Windows 用は `bin/plugin.exe` で、`publish.ps1` が生成します。プラグイン本体は `plugin/dev.mikanseilaboratory.obs.websocket.sdPlugin` です。

`INSTALL=1 ./publish.sh` は macOS の Stream Deck プラグインフォルダへコピーします。

## 使い方

1. キーのプロパティインスペクタで Send to から、そのキーが操作する OBS を選ぶ。すべての有効なインスタンス、グループ、1 台、または複数台を指定できる。
2. 同じインスペクタの Manage instances から接続設定ウィンドウを開く。OBS の追加、削除、並べ替え、グループはそこで行う。
3. 接続状態は設定ウィンドウの色で分かる。緑が接続済み、黄が接続中、赤が認証失敗、茶が到達不能。
4. 「すべての対象で同じパラメータ」を外すと、インスタンスごとのシーン名などを分けて保存できる。
5. 一覧に出る名前のうち、一部の OBS にしか無いものは警告される。
6. 長押しを有効にすると、配信や録画は停止、シーンはプレビューへのセット、ソースは非表示、ミュートはミュート固定になる。

キー下端の帯がインスタンスごとの状態です。6 台までは頭文字、それ以上は色だけを表示します。帯の色はインスタンスに付けた色です。

パスワードは Stream Deck のグローバル設定に保存されます。

## 構成

- `streamdeck-plugin` 0.1 が Stream Deck との WebSocket を担当する
- `obws` 0.15 が型付きの OBS WebSocket v5 クライアント
- Raw Request / Raw Batch だけは、`obws` が生リクエストを公開していないため別ソケットで op 6 / op 8 を送る
