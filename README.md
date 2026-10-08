# Deuteros i Rust

En ny udgave af Deuteros (1991) med tidssvarende 2D-grafik og asynkron, turbaseret online-multiplayer, der spilles i browseren. Mål, arkitektur og plan står i [docs/DESIGN.md](docs/DESIGN.md).

| Mappe | Indhold |
|---|---|
| `crates/deuteros-core` | Spillets regler: ren, deterministisk Rust uden motor eller I/O |
| `crates/deuteros-client` | Web-klienten (Bevy → WebAssembly) med procedurel grafik |
| `web/`, `scripts/` | HTML-side og build-script til browseren |
| `Godot/` | Godot-remaken [DeuterosOrg/Deuteros-Resurrected](https://github.com/DeuterosOrg/Deuteros-Resurrected), brugt som opslagsværk for regler og data |

## Kom i gang

Kræver [Rust](https://rustup.rs).

```bash
# Kerne-tests
cargo test -p deuteros-core

# Web-klienten (første build tager et stykke tid)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # skal matche Cargo.lock
scripts/build-web.sh
python3 -m http.server -d dist 8080                # åbn http://localhost:8080
```

Klienten kan også køre som desktop-app med `cargo run -p deuteros-client`.

Sådan henter du ændringer fra Godot-remaken:

```bash
git remote add upstream https://github.com/DeuterosOrg/Deuteros-Resurrected.git   # kun første gang
git fetch upstream && git merge upstream/develop
```
