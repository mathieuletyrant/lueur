# lueur

Syncs Philips Hue lights with the films played on Jellyfin, from a Linux server (Umbrel), with no Windows PC, no Hue Sync app and no Sync Box.

## How it works

1. **Analysis, once per film**: ffmpeg decodes the video (VA-API/Quick Sync on an Intel N100, VideoToolbox on macOS) and shrinks it to 96×54 on the GPU. lueur skips black bars and stores a 16×9 color grid, 12 times per second, in a compressed `.lueur` file (a few hundred KB for a 2-hour film).
2. **Playback**: lueur polls Jellyfin every 500 ms, extrapolates the position between two reports and keeps a locked local clock (re-synced only on a seek, a pause or a media change; small drifts are corrected gently).
3. **Lights**: each channel of the entertainment area takes the color of the part of the screen matching its position, streamed at 50 Hz through the Hue Entertainment API (DTLS 1.2 PSK, UDP 2100). When playback stops, the stream is closed and the lights return to their previous state.

During the film the load is close to zero: no decoding, just a lookup in the track.

## Installation

You need Rust, ffmpeg and OpenSSL. On macOS:

```sh
brew install rust ffmpeg openssl@3
git clone https://github.com/mathieuletyrant/lueur && cd lueur
OPENSSL_DIR="$(brew --prefix openssl@3)" cargo build --release
```

The binary is `target/release/lueur`.

## Usage

```sh
# 1. Pair (press the bridge's link button)
lueur pair 192.168.68.xx
# 2. List the entertainment areas and their channels
lueur areas
# 3. Check with a rainbow test pattern
lueur test --seconds 10
# 4. Analyze the library (skips films that are already up to date)
lueur analyze /downloads/movies /downloads/shows -j 2
# 5. Follow Jellyfin
lueur run
```

`lueur inspect track.lueur --at 754` prints the grid at a given time in the terminal.

## Films on another machine

lueur needs to read the video files for analysis, and the tracks must be in its `store` when playing. If the films live on a server (e.g. an Umbrel) and lueur runs elsewhere, mount the server's share (SMB) and use `path_map` to translate Jellyfin's paths to the mounted ones. On the first playback, lueur logs the path Jellyfin reports when no track is found.

## Configuration (`lueur.json`)

```json
{
  "store": "/data/tracks",
  "hue": { "bridge": "192.168.68.xx", "username": "…", "clientkey": "…", "area": "Living room TV" },
  "jellyfin": { "url": "http://192.168.68.58:8096", "api_key": "…", "device": "Apple TV" },
  "path_map": [{ "from": "/media", "to": "/downloads" }],
  "look": { "brightness": 1.0, "saturation": 1.15, "min_brightness": 0.0, "smoothing_ms": 60, "offset_ms": 0 },
  "auto_analyze": true
}
```

- `path_map`: paths as seen by Jellyfin → paths as seen by lueur.
- `jellyfin.device`: only follow one device (substring of its name or client).
- `look.offset_ms`: if the lights change late on cuts, increase it (e.g. 500).
- `auto_analyze`: a film played without a track is analyzed in the background.
- The Jellyfin API key must be an admin key (needed to get the file path).

## Tests

`cargo test` covers extraction (black bars, grid), the track format, the HueStream packet, channel/screen mapping, the clock and Jellyfin date parsing. The prototype was also tested end to end against a fake bridge (REST + `openssl s_server` in DTLS PSK mode) and a fake Jellyfin.

## To do

- Jellyfin websocket instead of polling, Radarr/Sonarr import webhook to analyze automatically.
- Web UI (live settings, offset from the couch, track preview).
- Umbrel app packaging (`mathieu-lueur`).
