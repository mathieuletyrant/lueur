# The HTTP API lives in the Rust binary, the UI is embedded in it

`lueur serve` runs the player, the Analysis queue and an axum HTTP API in one process, and serves the React UI from files embedded at build time. A separate backend (Node/TS) was considered and rejected: it would own no data of its own and would have to drive the player through a CLI or IPC, splitting state (live Look, now playing, queue progress) across two processes for no gain. One process also means one Docker image and one port, which is what an Umbrel app wants.

## Consequences

- The web build (`apps/web/dist`) must exist before `cargo build --release` to be embedded; debug builds read it from disk.
- No auth: the API trusts the LAN and the Umbrel `app_proxy`.
