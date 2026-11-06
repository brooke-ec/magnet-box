FROM docker.io/rust:slim-bookworm AS build

ARG pkg=magnet-box

WORKDIR /build

COPY . .

RUN apt-get update; \
    apt-get install --no-install-recommends -y pkg-config libssl-dev; \
    rm -rf /var/lib/apt/lists/*

RUN --mount=type=cache,target=/build/target \
    --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    set -eux; \
    cargo install cargo-patch --git https://github.com/itmettkeDE/cargo-patch --rev f99685ff --locked; \
    cargo patch; \
    cargo build --release; \
    objcopy --compress-debug-sections target/release/$pkg ./main

FROM docker.io/debian:bookworm-slim

WORKDIR /app

RUN mkdir -p data

## copy the main binary
COPY --from=build /build/main /app/main

## copy runtime assets
COPY --from=build /build/Rocket.toml ./Rocket.toml
COPY --from=build /build/static ./static
COPY --from=build /build/templates ./templates

## ensure the container listens globally on port 8080
ENV ROCKET_ADDRESS=0.0.0.0
ENV ROCKET_PORT=8080

ENV ROCKET_DATABASES='{storage={url="sqlite://data/magnet-box.db"}}'

CMD ["./main"]