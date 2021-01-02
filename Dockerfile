ARG ALPINE_VERSION=3.12
ARG RUST_VERSION=1-alpine${ALPINE_VERSION}

FROM rust:${RUST_VERSION} AS build
WORKDIR /usr/src/prometheus_wireguard_exporter

# Setup
ARG ARCH=x86_64
RUN apk add --update -q --no-cache musl-dev
RUN rustup target add ${ARCH}-unknown-linux-musl

# Install dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs
RUN cargo build --release && \
    rm -rf target/release/deps/prometheus_wireguard_exporter*

# Build the musl linked binary
COPY . .
RUN cargo build --release
RUN cargo install --target ${ARCH}-unknown-linux-musl --path .

FROM alpine:${ALPINE_VERSION}
EXPOSE 9586/tcp
RUN adduser prometheus-wireguard-exporter -s /bin/sh -D -u 1000 1000 && \
    mkdir -p /etc/sudoers.d && \
    echo 'prometheus-wireguard-exporter ALL=(root) NOPASSWD:/usr/bin/wg show * dump' > /etc/sudoers.d/prometheus-wireguard-exporter && \
    chmod 0440 /etc/sudoers.d/prometheus-wireguard-exporter
RUN apk add --update -q --no-cache wireguard-tools-wg sudo
USER prometheus-wireguard-exporter
ENTRYPOINT [ "prometheus_wireguard_exporter" ]
CMD [ "-a" ]
COPY --from=build --chown=prometheus-wireguard-exporter /usr/local/cargo/bin/prometheus_wireguard_exporter /usr/local/bin/prometheus_wireguard_exporter
