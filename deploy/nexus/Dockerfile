# syntax=docker/dockerfile:1
# Madrasa Nexus hub server image.
#
# Build from the repository root:
#   docker build -f deploy/nexus/Dockerfile -t madrasa-nexus .
#
# The image holds one binary, runs as a non-root user and checks its own health. All
# configuration comes from environment variables (see deploy/nexus/.env.example).
#
# cargo-chef builds the dependencies in their own layer, so a change to the hub's source
# recompiles the hub only.

FROM rust:1-slim-bookworm AS chef
RUN cargo install cargo-chef --locked --version 0.1.71
WORKDIR /src

FROM chef AS planner
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM chef AS build
COPY --from=planner /src/recipe.json recipe.json
RUN cargo chef cook --release --locked -p madrasa-nexus --recipe-path recipe.json
# The whole workspace is needed to resolve Cargo.lock; only the hub and the two library
# crates it uses (core, db) are compiled.
COPY . .
# Set by the release workflow so the hub reports the release tag as its version.
ARG MADRASA_RELEASE=""
RUN if [ -z "$MADRASA_RELEASE" ]; then unset MADRASA_RELEASE; fi; \
    cargo build --release --locked -p madrasa-nexus

FROM debian:bookworm-slim
RUN apt-get update \
    && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --system --uid 10001 --no-create-home --shell /usr/sbin/nologin nexus
COPY --from=build /src/target/release/madrasa-nexus /usr/local/bin/madrasa-nexus
USER 10001
# Default port. Cloud Run and Azure Container Apps set PORT, which the hub follows.
EXPOSE 4780
HEALTHCHECK --interval=30s --timeout=5s --start-period=30s --retries=3 \
    CMD ["madrasa-nexus", "healthcheck"]
ENTRYPOINT ["madrasa-nexus"]
