# Dependency/tool preparation is networked; execution services are offline.
FROM rust:1.97.1-slim@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c

# musl compiles the bundled SQLite C source; binutils verifies actual ELF
# headers. Package pins fail closed if the preparation mirror changes.
RUN apt-get update \
    && apt-get install -y --no-install-recommends \
        musl-tools=1.2.5-3.1~deb13u1 \
        musl-dev=1.2.5-3.1~deb13u1 \
        musl=1.2.5-3.1~deb13u1 \
        binutils=2.44-3 file=1:5.46-5 python3=3.13.5-1 git=1:2.47.3-0+deb13u1 \
    && rm -rf /var/lib/apt/lists/*
RUN rustup component add --toolchain 1.97.1 rustfmt clippy \
    && rustup target add --toolchain 1.97.1 x86_64-unknown-linux-musl

ENV RUSTUP_TOOLCHAIN=1.97.1 \
    RUSTUP_AUTO_INSTALL=0 \
    CARGO_BUILD_JOBS=4 \
    CARGO_HOME=/cache/cargo \
    CARGO_TARGET_DIR=/cache/target \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc \
    HOME=/cache/home
WORKDIR /src
