# Linux build and test environment for Open Task Manager (used locally through Colima and in CI).
#   docker build -f ci/linux-dev.Dockerfile -t otm-linux-dev .
FROM rust:1-bookworm

RUN apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
        pkg-config clang mold cmake \
        libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev mesa-vulkan-drivers \
        libx11-xcb-dev libxcb1-dev libxcb-xkb-dev libfontconfig-dev libfreetype-dev libzstd-dev \
        libssl-dev libasound2-dev libdbus-1-dev \
        xvfb xauth weston xdotool imagemagick dbus-x11 procps iproute2 systemd \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add rustfmt clippy
WORKDIR /src
