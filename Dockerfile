# NullNet: the game server with the web client built in. One image, one
# process, one SQLite file on a volume.
#
#   docker build -t nullnet .
#   docker run -p 8080:8080 -v nullnet-data:/data nullnet
#
# The web client is a WebAssembly build with fat LTO; the first build takes
# a while. Later builds reuse the cached dependency layers.

FROM rust:1.97-bookworm AS build
WORKDIR /src

RUN rustup target add wasm32-unknown-unknown \
    && apt-get update && apt-get install -y --no-install-recommends binaryen \
    && rm -rf /var/lib/apt/lists/*

# The dependency layers first, so a change to the game does not rebuild
# Bevy and axum.
COPY Cargo.toml Cargo.lock ./
COPY crates ./crates
RUN version=$(grep -A1 '^name = "wasm-bindgen"$' Cargo.lock | sed -n 's/^version = "\(.*\)"/\1/p') \
    && cargo install wasm-bindgen-cli --locked --version "$version"

COPY web ./web
COPY scripts ./scripts
RUN cargo build --release -p nullnet-server \
    && scripts/build-web.sh

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
