# Cadence build/test image (B-02).
#
# Pinned by digest; qualification runs offline after the preparation step
# fetches dependencies via the committed Cargo.lock. No `latest` tags.
FROM rust:1.97.1-slim@sha256:8e8cf8f7fd54a2d23d5a743b3a03f56e26b6c774276c33fa0595111704ebb15c

# PTY and process-control test prerequisites (P6); SQLite is bundled via
# rusqlite, so no system sqlite is required.
RUN apt-get update \
    && apt-get install -y --no-install-recommends procps ca-certificates \
    && rm -rf /var/lib/apt/lists/*

# The pinned toolchain file requests rustfmt and clippy; bake them in so
# gate services run offline (B-02: qualification executes without network).
RUN rustup component add rustfmt clippy
# Non-root execution for the app container profile (D-04).
ARG UID=1000
ARG GID=1000
RUN groupadd -g "${GID}" cadence || true \
    && useradd -m -u "${UID}" -g "${GID}" cadence || true

WORKDIR /src
