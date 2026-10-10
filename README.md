# NullNet

Et hacking-strategispil med asynkron, turbaseret online-multiplayer, der spilles i browseren. Et crew arbejder sig ind i et net af værter fra sit hjørne — scanner, bryder ind og planter bagdøre — mens The Legacy Net holder midten og slår ned på de crews, der larmer for meget. Flest data ved sidste tur vinder. Mål, regler og arkitektur står i [docs/DESIGN.md](docs/DESIGN.md); historien om omlægningen fra Deuteros-remake til hacking-spil i [docs/REDESIGN.md](docs/REDESIGN.md).

| Mappe | Indhold |
|---|---|
| `crates/nullnet-core` | Spillets regler: ren, deterministisk Rust uden motor eller I/O |
| `crates/nullnet-api` | JSON-typerne, server og klient udveksler |
| `crates/nullnet-client` | Web-klienten (Bevy → WebAssembly): kortet som graf og panelerne til ordrer |
| `crates/nullnet-sim` | Lader bot-crews spille mod hinanden og udskriver forløbet |
| `crates/nullnet-server` | Spilserveren: axum + SQLite. Invitationer, ordrer, turafvikling og web-klienten i én binærfil |
| `web/` | HTML-siden til Bevy-klienten og konsollen (`console.html`), som serveren serverer |
| `scripts/` | Build-script til browser-klienten |
| `Dockerfile`, `docker-compose.yml`, `fly.toml` | Hosting: ét image med server og klient, se [docs/HOSTING.md](docs/HOSTING.md) |

## Kom i gang

Kræver [Rust](https://rustup.rs).

```bash
# Kerne-tests
cargo test -p nullnet-core

# Se bot-crews spille et helt spil (--verbose viser hver begivenhed)
cargo run -p nullnet-sim -- --seed 7 --crews 3 --turns 50 --difficulty normal
# ... på et tilfældigt kort med ca. 60 værter
cargo run -p nullnet-sim -- --seed 7 --crews 3 --hosts 60

# Byg web-klienten og start serveren på din egen maskine, uden Docker (hurtigst på en laptop;
# scriptet siger, hvad der mangler). Åbn derefter http://localhost:8080/console
scripts/run-local.sh

# Eller kun serveren (konsollen virker uden web-klienten)
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

1. Opret et spil på `/console`: navn, crews (en bot kan spille et sæde), sværhedsgrad og sidste tur. Sæt kryds i *Practice*, så turen kører, så snart alle har afleveret — så kan et øvelsesspil mod en bot læres på en aften. Du kan også angive en frist i timer og en Discord- eller Slack-webhook, der får besked, når turene kører.
2. Send hvert crew sit invitationslink. Linket åbner kortet; `/console#<token>` er den rå konsol med alle ordrer som JSON.
3. Hver tur: klik en vært på kortet, vælg en operation for en hacker (scan, bryd ind, plant bagdør, stjæl data, forsvar), og tryk *Hand in*. Turen kører, når alle har afleveret, eller når fristen udløber. Loggen, en toast og en lyd fortæller, hvad der skete.
   Første gang fortæller klienten historien om ResetN00L, NullNet og The Legacy Net (spring over med *Skip*; *Help* har den igen). Guiden ved kortets fod foreslår det næste trin med en *Do it*-knap, der lægger ordren.
4. Spillet slutter på den sidste tur; flest point vinder (1 pr. data, 5 pr. vært man holder, 10 pr. vært befriet fra The Legacy Net).

## Hosting

`docker compose up -d --build` starter en server med web-klienten på port 8080 og databasen i en volume. `fly.toml` gør det samme på Fly.io. Se [docs/HOSTING.md](docs/HOSTING.md) for indstillinger, HTTPS, backup og notifikationer.
