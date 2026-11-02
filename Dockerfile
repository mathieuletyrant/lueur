# Image for the Umbrel (works with plain Docker too).
FROM node:22-slim AS web
RUN corepack enable
WORKDIR /src
COPY package.json pnpm-workspace.yaml pnpm-lock.yaml ./
COPY apps/web/package.json apps/web/
RUN pnpm install --frozen-lockfile
COPY apps/web apps/web
RUN pnpm --filter web build

FROM rust:1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY apps/server apps/server
# The UI is embedded in the binary (docs/adr/0001-api-in-rust-binary.md).
COPY --from=web /src/apps/web/dist apps/web/dist
RUN cargo build --release

FROM debian:bookworm-slim
# ffmpeg + Intel VA-API driver (iHD, amd64 only) for hardware decoding on the N100.
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg libssl3 ca-certificates \
    $([ "$(dpkg --print-architecture)" = amd64 ] && echo intel-media-va-driver) \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lueur /usr/local/bin/lueur
ENV LUEUR_CONFIG=/data/lueur.json LUEUR_PORT=8080
VOLUME /data
EXPOSE 8080
# Mount /dev/dri for VA-API: devices: ["/dev/dri:/dev/dri"]
# Mount the media read-only, e.g. -v /home/umbrel/umbrel/data/storage/downloads:/media:ro
ENTRYPOINT ["lueur"]
CMD ["serve"]
