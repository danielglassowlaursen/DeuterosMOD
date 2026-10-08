# NullNet

Et strategispil i et hacker-univers med asynkron, turbaseret online-multiplayer, der spilles i browseren. Reglerne bygger på Deuteros (Activision, 1991). Mål, arkitektur, ordbog og plan står i [docs/DESIGN.md](docs/DESIGN.md).

| Mappe | Indhold |
|---|---|
| `crates/nullnet-core` | Spillets regler: ren, deterministisk Rust uden motor eller I/O |
| `crates/nullnet-client` | Web-klienten (Bevy → WebAssembly) med procedurel grafik |
| `crates/nullnet-sim` | Lader bot-crews spille mod hinanden og udskriver tidslinjen |
| `web/`, `scripts/` | HTML-side og build-script til browseren |
| `Godot/` | Godot-remaken [DeuterosOrg/Deuteros-Resurrected](https://github.com/DeuterosOrg/Deuteros-Resurrected), brugt som opslagsværk for regler og data |

## Kom i gang

Kræver [Rust](https://rustup.rs).

```bash
# Kerne-tests
cargo test -p nullnet-core

# Se 3 bot-crews spille 3.000 dage (--verbose viser alt, hvad der bygges og flyttes)
cargo run -p nullnet-sim -- --seed 7 --crews 3 --days 3000

# Lav en afspilning, hvor du kan bladre gennem turene i browseren
cargo run --release -p nullnet-sim -- --crews 2 --days 1000 --json | python3 tools/replay.py > replay.html

# Web-klienten (første build tager et stykke tid)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # skal matche Cargo.lock
scripts/build-web.sh
python3 -m http.server -d dist 8080                # åbn http://localhost:8080
```

Klienten kan også køre som desktop-app med `cargo run -p nullnet-client`.

Sådan henter du ændringer fra Godot-remaken:

```bash
git remote add upstream https://github.com/DeuterosOrg/Deuteros-Resurrected.git   # kun første gang
git fetch upstream && git merge upstream/develop
```
