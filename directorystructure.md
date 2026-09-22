# ディレクトリ構成

```
crates/obs-pool/          N 台の OBS 接続、再接続、イベント、Raw チャネル
crates/multiobs-plugin/   Stream Deck プラグイン本体とアクション
pi/                       Property Inspector (React)
dev.flowingspdg.multiobs.rust.sdPlugin/
  manifest.json
  en.json / ja.json
  imgs/
  ui/                     Vite のビルド出力
  bin/                    publish スクリプトが配置する実行ファイル
publish.sh / publish.ps1
```
