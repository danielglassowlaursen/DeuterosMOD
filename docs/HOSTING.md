# Hosting

Serveren er én binærfil (`nullnet-server`), der holder spillene i én SQLite-fil og serverer web-klienten og konsollen selv. Den skal kun have en port, en mappe til databasen og helst HTTPS foran sig. Der er ingen konti: et spil oprettes på `/console`, og hver crew får et hemmeligt invitationslink.

## Indstillinger

| Miljøvariabel | Argument | Standard | Betydning |
|---|---|---|---|
| `PORT` | `--port` | `8080` | Porten serveren lytter på (altid på alle adresser) |
| `NULLNET_DB` | `--db` | `nullnet.db` | SQLite-filen med alle spil. Tag backup af den |
| `NULLNET_WEB` | `--web` | ingen | Mappen med den byggede web-klient (`dist/`). Uden den åbner invitationslinks konsollen |

Argumenterne vinder over miljøvariablerne.

## Docker (en VPS eller egen maskine)

```bash
docker compose up -d --build     # bygger billedet (første gang 20-30 minutter) og starter
docker compose logs -f           # "NullNet server on http://localhost:8080/console"
```

Buildet kompilerer web-klienten til WebAssembly inde i containeren. Cargos pakker og byggemappe gemmes i BuildKit-caches mellem builds, så efter første gang kompileres kun de crates, der er ændret: et rebuild efter en kodeændring tager et par minutter i stedet for hele buildet (`docker builder prune` rydder cachen, hvis den skal nulstilles). Som standard bruger det profilen `web-lite`, der linker i ca. 2,5 GB hukommelse (giv Docker mindst 4 GB). Har Docker 6 GB eller mere (Docker Desktop: Settings, Resources, Memory), giver `docker compose build --build-arg WEB_PROFILE=web` et modul, der er nogle megabyte mindre. Dør buildet med `SIGKILL` eller `cannot allocate memory`, er det hukommelsen: giv Docker mere, eller bliv ved `web-lite`.

Databasen ligger i volumen `nullnet-data`. Opdatering: `git pull && docker compose up -d --build`. Backup: `docker compose cp nullnet:/data/nullnet.db ./backup.db`.

Sæt en reverse proxy med HTTPS foran, fx Caddy med to linjer:

```
nullnet.example.org {
    reverse_proxy localhost:8080
}
```

Browser-notifikationer og klipbordet i konsollen kræver HTTPS (eller `localhost`).

## Fly.io

`fly.toml` ligger klar. Ret `app` og `primary_region`, og kør:

```bash
fly launch --no-deploy            # registrerer appen ud fra fly.toml
fly volumes create nullnet_data --size 1 --region <din region>
fly deploy                        # bygger Dockerfile'n hos Fly og starter én maskine
fly logs
```

Serveren skal køre som én maskine, der aldrig stopper (`auto_stop_machines = "off"`), fordi den kører fristerne hvert 30. sekund og holder databasen lokalt. Backup: `fly ssh console -C "cat /data/nullnet.db" > backup.db`.

## Uden Docker

```bash
cargo build --release -p nullnet-server
scripts/build-web.sh                                   # kræver wasm-bindgen-cli, se README
NULLNET_DB=/var/lib/nullnet/nullnet.db NULLNET_WEB=dist PORT=8080 target/release/nullnet-server
```

Kør den som en systemd-service med `Restart=always`, og lad en proxy tage HTTPS.

## Notifikationer

- **Webhook pr. spil.** Angiv en Discord- eller Slack-webhook-URL, når spillet oprettes. Serveren poster en linje, hver gang en tur er kørt, og når spillet er slut. Teksten nævner spillet, turen, dagen og hvem der har tur; invitationslinks sendes aldrig.
- **Browser.** Både kortklienten og konsollen kan vise en notifikation, når en tur kører, mens fanen er i baggrunden. Spilleren slår det til med knappen *Notify me*.
- **Lyd.** Kortklienten spiller en kort lyd, når en tur kører, en alarm ved angreb og en fanfare ved spillets slutning. Lyden starter først efter det første klik (browserens regel) og kan slås fra i toplinjen; valget huskes i browseren.

## Sikkerhed og drift

- Invitationslinket er crewets eneste nøgle. Send det kun til den, det er til.
- Konsollen på `/console` kan oprette spil for enhver, der kan nå serveren. Vil du begrænse det, så lad proxyen beskytte `/console` og `/api/games` (fx basic auth), og lad `/join/*` og `/api/crew/*` være åbne.
- Databasen er én fil i WAL-tilstand; stop serveren eller brug `sqlite3 .backup`, før du kopierer den under last.
- Serveren kører turene, hvis frister er udløbet, når den starter op igen, så nedetid koster ingen ture.
