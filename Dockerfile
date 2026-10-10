# syntax=docker/dockerfile:1
#
# NullNet: the game server with the web client built in. One image, one
# process, one SQLite file on a volume.
#
#   docker build -t nullnet .
#   docker run -p 8080:8080 -v nullnet-data:/data nullnet
#
# The web client is a WebAssembly build that takes a while the first time.
# Cargo's registry and build directory are kept in BuildKit caches between
# builds, so after the first one only the crates that changed are compiled
# again: a rebuild after a code change takes minutes, not the full build.
# (`docker builder prune` clears those caches if they ever need a reset.)
#
# WEB_PROFILE picks the Cargo profile for the client: `web-lite` (the
# default) links in about 2.5 GB of memory; `web` makes a smaller module but
# needs 5-6 GB, so give Docker that much before choosing it:
#
#   docker build --build-arg WEB_PROFILE=web -t nullnet .

FROM rust:1.97-bookworm AS build
ARG WEB_PROFILE=web-lite
WORKDIR /src

RUN rustup target add wasm32-unknown-unknown \
    && apt-get update && apt-get install -y --no-install-recommends binaryen \
    && rm -rf /var/lib/apt/lists/*

# The tool for the web build first: it changes only with Cargo.lock.
COPY Cargo.lock ./
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    version=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p') \
    && cargo install wasm-bindgen-cli --locked --version "$version"

COPY Cargo.toml ./
COPY crates ./crates
COPY web ./web
COPY scripts ./scripts

# Two steps, so a server that built stays built if the web client's link
# runs out of memory. The build directory is a cache mount, which is gone
# once the step ends, so the server binary is copied out within the step.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target,sharing=locked \
    cargo build --release -p nullnet-server \
    && mkdir -p /out \
    && cp target/release/nullnet-server /out/nullnet-server

# The client lands in dist/, outside the cached build directory, so it stays
# in the image layer.
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=/src/target,sharing=locked \
    scripts/build-web.sh "$WEB_PROFILE"

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data /app
COPY --from=build /out/nullnet-server /usr/local/bin/nullnet-server
COPY --from=build /src/dist /app/dist
ENV PORT=8080 NULLNET_DB=/data/nullnet.db NULLNET_WEB=/app/dist
VOLUME /data
EXPOSE 8080
CMD ["nullnet-server"]
