# NullNet: the game server with the web client built in. One image, one
# process, one SQLite file on a volume.
#
#   docker build -t nullnet .
#   docker run -p 8080:8080 -v nullnet-data:/data nullnet
#
# The web client is a WebAssembly build that takes a while the first time.
# WEB_PROFILE picks the Cargo profile for it: `web-lite` (the default) links
# in about 2.5 GB of memory; `web` makes a smaller module but needs 5-6 GB,
# so give Docker that much before choosing it:
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
RUN version=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p') \
    && cargo install wasm-bindgen-cli --locked --version "$version"

COPY Cargo.toml ./
COPY crates ./crates
COPY web ./web
COPY scripts ./scripts
# Two steps, so a server that built stays built if the web client's link
# runs out of memory.
RUN cargo build --release -p nullnet-server
RUN scripts/build-web.sh "$WEB_PROFILE"

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y --no-install-recommends ca-certificates \
    && rm -rf /var/lib/apt/lists/* \
    && mkdir -p /data /app
COPY --from=build /src/target/release/nullnet-server /usr/local/bin/nullnet-server
COPY --from=build /src/dist /app/dist
ENV PORT=8080 NULLNET_DB=/data/nullnet.db NULLNET_WEB=/app/dist
VOLUME /data
EXPOSE 8080
CMD ["nullnet-server"]
