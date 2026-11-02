# lueur

Syncs Philips Hue lights with the films played on Jellyfin, from a Linux server (Umbrel), with no Windows PC, no Hue Sync app and no Sync Box.

## How it works

1. **Analysis, once per film**: ffmpeg decodes the video (VA-API/Quick Sync on an Intel N100, VideoToolbox on macOS) and shrinks it to 96×54 on the GPU. lueur skips black bars and stores a 16×9 color grid, 12 times per second, in a compressed `.lueur` file (a few hundred KB for a 2-hour film).
2. **Playback**: lueur polls Jellyfin every 500 ms, extrapolates the position between two reports and keeps a locked local clock (re-synced only on a seek, a pause or a media change; small drifts are corrected gently).
3. **Lights**: each channel of the entertainment area takes the color of the part of the screen matching its position, streamed at 50 Hz through the Hue Entertainment API (DTLS 1.2 PSK, UDP 2100). When playback stops, the stream is closed and the lights return to their previous state.

During the film the load is close to zero: no decoding, just a lookup in the track.

## Repository

```
apps/server/   Rust: analysis, player, Hue/Jellyfin clients, HTTP API (`lueur serve`)
apps/web/      React + Vite + TypeScript UI, embedded in the binary at build time
docs/adr/      Architecture decisions
GLOSSARY.md    Domain vocabulary (Bridge, Area, Library, Scan, Track…)
```

## Docker (Umbrel)

```sh
docker build -t lueur .
docker run -d --name lueur -p 8080:8080 \
  -v lueur-data:/data \
  -v /home/umbrel/umbrel/data/storage/downloads:/media:ro \
  --device /dev/dri:/dev/dri \
  lueur
```

Open `http://<host>:8080` and follow Setup: Bridge → Pairing → Area → Jellyfin → Libraries, then **Scan now** in Library. The config and the tracks live in `/data`.

Bridge discovery uses the Hue cloud (works from inside Docker) and mDNS (needs `--network host`); you can always type the IP.

## Development

You need Rust, ffmpeg, OpenSSL, Node 22 and pnpm. On macOS:

```sh
brew install rust ffmpeg openssl@3 pnpm
pnpm install
OPENSSL_DIR="$(brew --prefix openssl@3)" cargo run -- serve   # API + UI on :8080
pnpm dev                                                       # UI with hot reload on :5173, proxies /api to :8080
pnpm build                                                     # UI, then the release binary with the UI inside
pnpm check                                                     # typecheck + cargo test
```

Debug builds read the UI from `apps/web/dist`, release builds embed it.

## CLI

The UI covers the usual flow; the CLI stays for scripts and debugging.

```sh
lueur serve                                   # web UI + player (Docker default)
lueur pair 192.168.68.xx                      # pair (press the bridge's link button)
lueur areas                                   # list entertainment areas and channels
lueur test --seconds 10                       # rainbow test pattern
lueur analyze /downloads/movies -j 2          # analyze a folder (skips up-to-date tracks)
lueur run                                     # player only, no UI
lueur inspect track.lueur --at 754            # print the grid at a given time
```

## Configuration (`lueur.json`)

Written by the UI; editing it by hand works too.

```json
{
  "store": "/data/tracks",
  "hue": { "bridge": "192.168.68.xx", "username": "…", "clientkey": "…", "area": "Living room TV" },
  "jellyfin": { "url": "http://192.168.68.58:8096", "api_key": "…", "device": "Apple TV" },
  "libraries": [{ "name": "Movies", "jellyfin": "/media/movies", "path": "/media/movies" }],
  "look": { "brightness": 1.0, "saturation": 1.15, "min_brightness": 0.0, "smoothing_ms": 60, "offset_ms": 0 },
  "auto_analyze": true,
  "jobs": 1
}
```

- `libraries`: Jellyfin folder → the same folder as seen by lueur (they differ when Jellyfin runs in another container). `path_map` (`[{ "from", "to" }]`) still works for extra mappings.
- `store`: defaults to `tracks/` next to the config file.
- `jellyfin.device`: only follow one device (substring of its name or client).
- `look.offset_ms`: if the lights change late on cuts, increase it (e.g. 500).
- `auto_analyze`: a film played without a track is queued for analysis.
- The Jellyfin API key must be an admin key (needed to get the file path).

## Tests

`cargo test` covers extraction (black bars, grid), the track format, the HueStream packet, channel/screen mapping, the clock, Jellyfin date parsing, path mapping, the analysis queue and config patches. The whole stack was also run end to end against a fake bridge (REST + `openssl s_server` in DTLS PSK mode) and a fake Jellyfin.

## To do

- Radarr/Sonarr import webhook to Scan automatically; Jellyfin websocket instead of polling.
- Umbrel app packaging (`mathieu-lueur`).
