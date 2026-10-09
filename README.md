# NullNet

Et strategispil i et hacker-univers med asynkron, turbaseret online-multiplayer, der spilles i browseren. Reglerne bygger på Deuteros (Activision, 1991). Mål, arkitektur, ordbog og plan står i [docs/DESIGN.md](docs/DESIGN.md).

| Mappe | Indhold |
|---|---|
| `crates/nullnet-core` | Spillets regler: ren, deterministisk Rust uden motor eller I/O |
| `crates/nullnet-api` | JSON-typerne, server og klient udveksler |
| `crates/nullnet-client` | Web-klienten (Bevy → WebAssembly): netværkskortet som spillebræt og terminalpaneler til ordrer |
| `crates/nullnet-sim` | Lader bot-crews spille mod hinanden og udskriver tidslinjen |
| `crates/nullnet-server` | Spilserveren: axum + SQLite. Invitationer, ordrer, turafvikling og web-klienten i én binærfil |
| `web/` | HTML-siden til Bevy-klienten og konsollen (`console.html`), som serveren serverer |
| `scripts/` | Build-script til browser-klienten |
| `Dockerfile`, `docker-compose.yml`, `fly.toml` | Hosting: ét image med server og klient, se [docs/HOSTING.md](docs/HOSTING.md) |
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

# Start en spilserver og opret et spil på http://localhost:8080/console
cargo run -p nullnet-server -- --db nullnet.db --port 8080

# Med web-klienten bygget (se nedenfor) åbner invitationslinkene kortet i stedet for konsollen
cargo run -p nullnet-server -- --db nullnet.db --port 8080 --web dist

# Web-klienten (første build tager et stykke tid)
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli --version 0.2.129   # skal matche Cargo.lock
scripts/build-web.sh                              # eller: scripts/build-web.sh web-lite på en maskine med lidt hukommelse
python3 -m http.server -d dist 8080                # åbn http://localhost:8080
```

Klienten kan også køre som desktop-app: `cargo run -p nullnet-client -- --server http://localhost:8080 --token <token>`.

## Sådan spiller man

1. Opret et spil på `/console`: navn, crews (en bot kan spille et sæde), dage pr. tur, frist i timer, sidste dag og eventuelt en Discord- eller Slack-webhook, der får besked, når turene kører.
2. Send hvert crew sit invitationslink. Linket åbner kortet; `/console#<token>` er den rå konsol med alle ordrer som JSON.
3. Hver tur: læg ordrer med panelerne og tryk *Hand in*. Turen kører, når alle har afleveret, eller når fristen udløber. Loggen, en toast og en lyd fortæller, hvad der skete; kampe kan afspilles.
   Første gang fortæller klienten historien om ResetN00L, NullNet og The Legacy Net i fem sider, læst op af en fortællerstemme (spring over med *Skip*; *Help* har den igen). Guiden over kortet viser det næste trin i åbningen (rekruttér, taps, dropper, citadel, orm, anden vært, oprustning) med de knapper, der skal trykkes på; *Help* i toplinjen har alle trin og reglerne i korte træk.
4. Spillet slutter, når en crew holder det meste af hjemmenettet, eller på den sidste dag, hvor flest point vinder.

## Hosting

`docker compose up -d --build` starter en server med web-klienten på port 8080 og databasen i en volume. `fly.toml` gør det samme på Fly.io. Se [docs/HOSTING.md](docs/HOSTING.md) for indstillinger, HTTPS, backup og notifikationer.

Sådan henter du ændringer fra Godot-remaken:

```bash
git remote add upstream https://github.com/DeuterosOrg/Deuteros-Resurrected.git   # kun første gang
git fetch upstream && git merge upstream/develop
```
