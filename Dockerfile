# Image for the Umbrel (not tested yet).
FROM rust:1-slim-bookworm AS build
RUN apt-get update && apt-get install -y --no-install-recommends pkg-config libssl-dev && rm -rf /var/lib/apt/lists/*
WORKDIR /src
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
# ffmpeg + Intel VA-API driver (iHD) for hardware decoding on the N100.
RUN apt-get update && apt-get install -y --no-install-recommends ffmpeg intel-media-va-driver libssl3 ca-certificates \
    && rm -rf /var/lib/apt/lists/*
COPY --from=build /src/target/release/lueur /usr/local/bin/lueur
ENV LUEUR_CONFIG=/data/lueur.json
VOLUME /data
# Mount /dev/dri for VA-API: devices: ["/dev/dri:/dev/dri"]
ENTRYPOINT ["lueur"]
CMD ["run"]
