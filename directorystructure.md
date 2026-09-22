# ディレクトリ構成

```
crates/obs-pool/          N 台の OBS 接続、再接続、イベント、Raw チャネル
crates/multiobs-plugin/   Stream Deck プラグイン本体とアクション
pi/                       Property Inspector (React)
plugin/dev.mikanseilaboratory.obs.websocket.sdPlugin/
  manifest.json
  en.json / ja.json
  images/
  propertyinspector/      Vite のビルド出力
  bin/plugin              macOS ユニバーサルバイナリ
  bin/plugin.exe          Windows 実行ファイル
publish.sh / publish.ps1
```
