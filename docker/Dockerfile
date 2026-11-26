# syntax=docker/dockerfile:1@sha256:4edf897a3ffa55b89f906fc8cc78afdb3f1834cc9c7083565e611a8a7d5fe99e
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
        openjdk-21-jre-headless=21.0.13.1+2-1~deb13u1 \
    && rm -rf /var/lib/apt/lists/*
# TLC 1.7.4 for the model gate (tools/tlc_gate.py, #28); the same digest the
# handover pinned in tools/run_models.py.
ADD --checksum=sha256:936a262061c914694dfd669a543be24573c45d5aa0ff20a8b96b23d01e050e88 https://github.com/tlaplus/tlaplus/releases/download/v1.7.4/tla2tools.jar /opt/tla/tla2tools.jar
RUN chmod 0444 /opt/tla/tla2tools.jar
RUN rustup component add --toolchain 1.97.1 rustfmt clippy \
    && rustup target add --toolchain 1.97.1 x86_64-unknown-linux-musl

ENV RUSTUP_TOOLCHAIN=1.97.1 \
    RUSTUP_AUTO_INSTALL=0 \
    CARGO_BUILD_JOBS=4 \
    CARGO_HOME=/cache/cargo \
    CARGO_TARGET_DIR=/cache/target \
    CARGO_TARGET_X86_64_UNKNOWN_LINUX_MUSL_LINKER=musl-gcc \
    CC_x86_64_unknown_linux_musl=musl-gcc \
    HOME=/cache/home \
    TLA2TOOLS_JAR=/opt/tla/tla2tools.jar \
    CADENCE_EVIDENCE_DIR=/cache/evidence-out
WORKDIR /src
