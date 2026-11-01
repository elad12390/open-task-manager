# Ubuntu 22.04 build environment for the .deb packages: the oldest glibc we support, so the binary
# runs on newer distributions too. Used locally through Colima and by the release workflow.
#   docker build -f ci/linux-build-2204.Dockerfile -t otm-linux-build-2204 .
FROM ubuntu:22.04

ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
        ca-certificates curl build-essential pkg-config clang cmake git \
        libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev mesa-vulkan-drivers \
        libx11-xcb-dev libxcb1-dev libxcb-xkb-dev libfontconfig-dev libfreetype-dev libzstd-dev \
        libssl-dev libasound2-dev libdbus-1-dev \
        xvfb xdotool imagemagick dbus-x11 procps iproute2 \
    && rm -rf /var/lib/apt/lists/*

ENV RUSTUP_HOME=/usr/local/rustup CARGO_HOME=/usr/local/cargo PATH=/usr/local/cargo/bin:$PATH
RUN curl -sSf https://sh.rustup.rs | sh -s -- -y --profile minimal --default-toolchain stable -c rustfmt -c clippy
WORKDIR /src
