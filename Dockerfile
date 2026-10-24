FROM rust:alpine AS builder

WORKDIR /src
COPY . .

RUN cargo build --release

FROM alpine:latest

WORKDIR /magnet-box

COPY --from=builder /src/target/release/magnet-box .
COPY templates templates

CMD ["./magnet-box"]