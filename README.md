# Multi OBS

Stream Deck plugin for any number of named OBS Studio instances. Add, remove, and reorder them. Each action targets all enabled instances, a group, or a selection.

## Requirements

- Stream Deck 6.4+
- OBS Studio 30.2+ (obs-websocket 5.5, bundled with OBS)
- WebSocket server on for each OBS, each on its own port

## Build

```bash
cargo test --workspace
./publish.sh
```

macOS: `publish.sh` writes a universal `bin/plugin` (arm64 and x64). Windows: `publish.ps1` writes `bin/plugin.exe`. The bundle is `plugin/dev.mikanseilaboratory.obs.websocket.sdPlugin`.

`INSTALL=1 ./publish.sh` copies it into the macOS Stream Deck plugin folder.

## Usage

1. In the key inspector, choose **Send to**: all enabled instances, a group, or specific instances.
2. **Manage instances** in the same inspector adds, removes, reorders, and groups connections.
3. Status colors: green connected, yellow connecting, red auth failed, brown unreachable.
4. Turn off “same parameters for every target” to store per-instance values, such as scene names.
5. Names missing on some instances are flagged.
6. Long press stops streaming and recording, sets the scene to preview, hides the source, and holds mute on.

A strip under the key shows each instance. Up to six show initials; more than that show color only. The color is the one set on the instance.

Passwords are stored in Stream Deck global settings.

## Layout

- `streamdeck-plugin` 0.1 talks to Stream Deck.
- `obws` 0.15 is the typed OBS WebSocket v5 client.
- Raw Request and Raw Batch use a separate socket (op 6 and op 8). `obws` does not expose raw requests.

## License

[MIT](LICENSE-MIT) OR [Apache-2.0](LICENSE-APACHE).
