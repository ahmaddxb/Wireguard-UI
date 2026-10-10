# ==============================================================================
# Stage 1: Build self-contained Rust binary & embedded web assets
# ==============================================================================
FROM rust:alpine AS builder

RUN apk add --no-cache musl-dev gcc sqlite-dev
WORKDIR /usr/src/wireguard-ui

COPY . .
RUN cargo build --release

# ==============================================================================
# Stage 2: Minimal Ultra-low Footprint Runtime Container (~18MB Image Size)
# ==============================================================================
FROM alpine:3.20

RUN apk add --no-cache wireguard-tools iptables iproute2 ca-certificates bash curl openssh-client iperf3

WORKDIR /app

COPY --from=builder /usr/src/wireguard-ui/target/release/wireguard-ui /app/wireguard-ui

ENV PORT=8080
ENV RUST_LOG=wireguard_ui=info

EXPOSE 8080/tcp
EXPOSE 51820/udp
EXPOSE 5201/tcp
EXPOSE 5201/udp

CMD ["/app/wireguard-ui"]
