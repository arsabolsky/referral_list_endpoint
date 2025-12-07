ARG VARIANT="bullseye"
FROM mcr.microsoft.com/vscode/devcontainers/rust:1-${VARIANT}

# install additional packages.
RUN apt-get update && export DEBIAN_FRONTEND=noninteractive \
    && apt-get -y install --no-install-recommends \
        libssl-dev \
        build-essential \
        pkg-config \
        ca-certificates \
        git \
        gcc-mingw-w64-x86-64 \

    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

