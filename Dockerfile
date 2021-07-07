ARG BUILDPLATFORM=linux/amd64

ARG ALPINE_VERSION=3.12
ARG RUST_VERSION=1-slim-bullseye

FROM --platform=${BUILDPLATFORM} rust:${RUST_VERSION} AS build
WORKDIR /usr/src/prometheus_wireguard_exporter

# Setup
RUN apt-get update -y && \
    apt-get install -y \
    # to cross build with musl
    musl-tools \
    # to download the musl cross build tool
    wget \
    # for verifying the binary properties
    file

ARG TARGETPLATFORM
RUN echo "Setting variables for ${TARGETPLATFORM:=linux/amd64}" && \
    case "${TARGETPLATFORM}" in \
      linux/amd64) \
        MUSL="x86_64-linux-musl"; \
        RUSTTARGET="x86_64-unknown-linux-musl"; \
        break;; \
      linux/arm64) \
        MUSL="aarch64-linux-musl"; \
        RUSTTARGET="aarch64-unknown-linux-musl"; \
        break;; \
      linux/arm/v7) \
        MUSL="armv7m-linux-musleabi"; \
        RUSTTARGET="armv7-unknown-linux-musleabi"; \
        break;; \
      linux/arm/v6) \
        MUSL="armv6-linux-musleabi"; \
        RUSTTARGET="arm-unknown-linux-musleabi"; \
        break;; \
      linux/386) \
        MUSL="i686-linux-musl"; \
        RUSTTARGET="i686-unknown-linux-musl"; \
        break;; \
      linux/ppc64le) \
        MUSL="powerpc64le-linux-musl"; \
        RUSTTARGET="powerpc64le-unknown-linux-musl"; \
        break;; \
      linux/s390x) \
        MUSL="s390x-linux-musl"; \
        RUSTTARGET="s390x-unknown-linux-musl"; \
        break;; \
      linux/riscv64) \
        MUSL="riscv64-linux-musl"; \
        RUSTTARGET="riscv64gc-unknown-linux-musl"; \
        break;; \
      *) echo "unsupported platform ${TARGETPLATFORM}"; exit 1;; \
    esac && \
    echo "${MUSL}" | tee /tmp/musl && \
    echo "${RUSTTARGET}" | tee /tmp/rusttarget

RUN MUSL="$(cat /tmp/musl)" && \
    wget -qO- "https://musl.cc/$MUSL-cross.tgz" | tar -xzC /tmp && \
    rm "/tmp/$MUSL-cross/usr" && \
    cp -fr /tmp/"$MUSL"-cross/* / && \
    rm -rf "/tmp/$MUSL-cross"

RUN rustup target add "$(cat /tmp/rusttarget)"

# Copy .cargo/config for cross build configuration
COPY .cargo ./.cargo

# Install dependencies
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && \
    echo "fn main() {}" > src/main.rs
RUN CC="$(cat /tmp/musl)-gcc" cargo build --target "$(cat /tmp/rusttarget)" --release && \
    rm -rf target/release/deps/prometheus_wireguard_exporter*

# Build static binary with musl built-in
COPY . .
RUN CC="$(cat /tmp/musl)-gcc" cargo build --target "$(cat /tmp/rusttarget)" --release && \
    mv target/*-linux-*/release/prometheus_wireguard_exporter /tmp/binary
RUN file /tmp/binary

# Test the binary works on the target platform
FROM scratch AS binarytest
COPY --from=build /tmp/binary /binary
RUN ["/binary", "--help"]

FROM alpine:${ALPINE_VERSION}
EXPOSE 9586/tcp
WORKDIR /usr/local/bin
RUN adduser prometheus-wireguard-exporter -s /bin/sh -D -u 1000 1000 && \
    mkdir -p /etc/sudoers.d && \
    echo 'prometheus-wireguard-exporter ALL=(root) NOPASSWD:/usr/bin/wg show * dump' > /etc/sudoers.d/prometheus-wireguard-exporter && \
    chmod 0440 /etc/sudoers.d/prometheus-wireguard-exporter
RUN apk add --update -q --no-cache wireguard-tools-wg sudo
USER prometheus-wireguard-exporter
ENTRYPOINT [ "/usr/local/bin/prometheus_wireguard_exporter" ]
CMD [ "-a" ]
COPY --from=binarytest --chown=prometheus-wireguard-exporter /binary ./prometheus_wireguard_exporter
