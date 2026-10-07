FROM rust:1.89-slim-bookworm AS builder

RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
		pkg-config \
		libudev-dev \
		libusb-1.0-0-dev \
	&& rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml ./
COPY Cargo.lock ./
COPY src ./src

RUN cargo build --release --locked

FROM debian:bookworm-slim

RUN apt-get update \
	&& apt-get install -y --no-install-recommends \
		ca-certificates \
		libudev1 \
		libusb-1.0-0 \
	&& rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/unraid-mcp2221-screen /usr/local/bin/unraid-mcp2221-screen

ENTRYPOINT ["/usr/local/bin/unraid-mcp2221-screen"]
