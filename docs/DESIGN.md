# NullNet: design

NullNet er et strategispil i et hacker-univers med asynkron, turbaseret online-multiplayer, der spilles i browseren. Reglerne bygger på Deuteros (Activision, 1991). Udgangspunktet er Godot-remaken [DeuterosOrg/Deuteros-Resurrected](https://github.com/DeuterosOrg/Deuteros-Resurrected), som ligger i `Godot/` og bruges som opslagsværk for regler og data.

Temaet blev udviklet i brainstorm-dokumentet [Deuteros: hacker-univers](https://claude.ai/artifact/WZsx81LvTRyA4suCaAUH8v). Dokumentet er privat, indtil ejeren deler det.

## Verden

Det første internet brød sammen i Nedbruddet, da en AI gik amok og tog kontrollen over det. Menneskeheden byggede et nyt net fra bunden og kaldte det NullNet. Hver spiller leder en hacker-crew, der genopretter forbindelsen til glemte netværk og tager dem i besiddelse, før rivalerne gør det. AI'en døde dog ikke helt: resterne af den lever videre i de glemte servere som **The Legacy Net**.

## Beslutninger

| Emne | Valg |
|---|---|
| Multiplayer | Online og asynkront, med samtidige ture. Serveren gemmer spillet. |
| Spilform | 2-4 crews konkurrerer, og The Legacy Net er en fælles AI-modstander. |
| Crew mod crew | Med fra første version. Ingen angreb mellem crews i de første 5 ture. |
| Tema | Ren hacker. Rigtige hacker-udtryk, men reglerne er et spil. |
| Grafik | Moderne 2D i mørk neon med terminal-look, tegnet procedurelt med shaders. |
| Platform | Browser (WebAssembly). |
| Teknik | Rust med [Bevy](https://bevyengine.org) 0.19 til klienten og axum til serveren. |

## Ordbog: fra Deuteros til NullNet

Reglerne er Deuteros', men alle navne i spillet og i koden er hacker-navne. Kolonnen "Kode" er navnet i Rust.

**Kortet, baserne og folkene**

| Deuteros | NullNet | Kode |
|---|---|---|
| Stjernesystem (9) | Netværk | `Network` |
| Solen | Backbone | |
| Planet (44) | Vært | `Host` |
| Måne (116) | Undersystem | `Host` med en forælder |
| Asteroide | Forladt datacache | `Cache` |
| Jorden | Skjulestedet (én pr. crew) | `Hideout` |
| Jordbase (2 dele) | Bagdør (2 dele) | `backdoor` |
| Rumstation (8 sektioner) | Citadel (8 moduler) | `Citadel` |
| Methanoids | The Legacy Net | `Legacy` |
| Forskere: Technician, Doctor, Professor | Analytikere: Script kiddie, Hacker, Elite | `StaffKind::Analyst` |
| Produktionsfolk: Apprentice, Engineer, Expert | Kodere: Junior, Udvikler, Arkitekt | `StaffKind::Coder` |
| Marinesoldater: Pilot, Captain, Admiral | Operatører: Runner, Ghost, Phantom | `StaffKind::Operator` |
| Shuttle | Dropper | `Dropper` |
| IOS | Orm | `Worm` |
| SCG | Tunnelskib | `Tunneler` |

**Ressourcer**

| Deuteros | NullNet | Kode |
|---|---|---|
| Iron | Regnekraft | `Compute` |
| Titanium | Lagerplads | `Storage` |
| Aluminium | Hukommelse | `Memory` |
| Carbon | Kildekode | `Code` |
| Copper | Adgangskoder | `Credentials` |
| Hydrogen | Båndbredde | `Bandwidth` |
| Deuterium | Exit-noder | `ExitNodes` |
| Methane | Proxyer | `Proxies` |
| Helium | Krypteringsnøgler | `Keys` |
| Palladium | Zero-day-fragmenter | `ZeroDays` |
| Platinum | Kryptovaluta | `Crypto` |
| Silver | Certifikater | `Certificates` |
| Gold | Signeringsnøgler | `SigningKeys` |
| Silica | Firmware | `Firmware` |
| MeH-brændstof (hydrogen + methane) | Proxykæder (båndbredde + proxyer) | `ProxyChains` |
| HeD-brændstof (helium + deuterium) | Onion-ruter (nøgler + exit-noder) | `OnionRoutes` |

**Genstande**

| Deuteros | NullNet | Kode |
|---|---|---|
| Derrick | Tap | `Tap` |
| S chassis, S drive | Dropper-kerne, dropper-motor | `DropperCore`, `DropperEngine` |
| I chassis, I drive | Orme-kerne, orme-motor | `WormCore`, `WormEngine` |
| G chassis, star drive | Tunnel-kerne, tunnel-motor | `TunnelCore`, `TunnelEngine` |
| OF frame | Citadel-modul | `CitadelModule` |
| R frame | Bagdørssæt | `BackdoorKit` |
| Supply, tool, cryo pod | Datacontainer, værktøjsmodul, session-pod | `DataContainer`, `ToolModule`, `SessionPod` |
| ACC | Exfil-script | `ExfilScript` |
| AOC | Build-bot | `BuildBot` |
| MTX | Krypteret link | `EncryptedLink` |
| Bandaid | Patch | `Patch` |
| SDM | Kill switch | `KillSwitch` |
| Grapple | Sniffer | `Sniffer` |
| AMA | Crawler | `Crawler` |
| DFCC | C2-controller | `C2Controller` |
| IOS drone | Daemon | `Daemon` |
| Star drone | Jæger-daemon | `HunterDaemon` |
| PTL | Logikbombe | `LogicBomb` |
| Torpedo launcher | Exploit-launcher | `ExploitLauncher` |
| Hyperlight | Kvantelink | `QuantumLink` |
| MFL | Forstærker | `Amplifier` |
| Comms pod | Protokol-adapter | `ProtocolAdapter` |
| Prison pod | Honeypot | `Honeypot` |
| Sonic blaster | Jammer | `Jammer` |
| Pulse blaster laser | Legacy-exploit | `LegacyExploit` |
| Alien artifact | Kildekode-fragment | `SourceFragment` |

## Arkitektur

```
crates/
  nullnet-core      Spillets regler. Ren Rust uden motor, ur eller I/O.
  nullnet-server    (M2) axum + SQLite. Autoritativ: gemmer spil og afvikler ture.
  nullnet-client    Bevy, kompileret til WebAssembly. Serveres af serveren.
tools/              Konverter, der henter datatabellerne ud af Godot-koden.
Godot/              Godot-remaken: opslagsværk for regler og data.
```

### nullnet-core

- **Deterministisk.** Samme verden og samme ordrer giver altid samme resultat, på alle platforme. Derfor bruges kun heltal, kun ordnede samlinger (`BTreeMap`) og en egen PCG32-generator (`Rng`). Generatorens tilstand gemmes i verdenen, så en tur altid kan genafspilles. Den kommer ikke fra `rand`, fordi en opgradering af den pakke kunne ændre tallene.
- **To slags data.** `GameData` er de faste regeltabeller (genstande, forskning, netværk og værter), og `World` er alt, der ændrer sig. Tabellerne ligger i `crates/nullnet-core/data/` og er trukket ud af `CoreData.cs` med `tools/extract_coredata.py`. Et gemt spil er en `World` serialiseret med serde.
- **Ordrer.** `Command` er en ordre til den kommende tur. En ugyldig ordre springes over og står i turrapporten. Den stopper aldrig turen.
- **Afvikling.** `resolve_turn(data, world, orders, days)` udfører ordrerne og kører derefter `days` dage. Hver dag kører systemerne i originalens rækkefølge: unlocks, minedrift, træning, produktion, transport, forskning, Legacy-daemons, Legacy-transmissioner og krypterede links.
- **Klienten bruger samme crate.** Så kan den vise, hvad en ordre vil gøre, før turen sendes. Resultatet bestemmes altid af serveren.

### nullnet-server (M2)

- Opretter spil og laver et invitationslink pr. crew, med et hemmeligt token pr. crew i linket.
- Modtager ordrer og afvikler turen, når alle har afleveret, eller når fristen udløber.
- Gemmer hver tur i SQLite: verdenen før turen, ordrerne og rapporten. Så kan man spole tilbage og finde fejl.
- Sender hver crew kun det, den har scannet (fog of war). Modstandernes ordrer sendes aldrig.
- Giver besked, når en ny tur er klar. Første version gør det i browseren, senere eventuelt også med e-mail eller push.
- Én binærfil, der også serverer web-klienten, så den er let at hoste.

### nullnet-client

- Bevy 0.19 i browseren via WebGL2. Bygges med `scripts/build-web.sh`.
- Al grafik genereres i WGSL-shaders, så klienten ikke har billedfiler. Den nuværende oversigt er stadig rum-udgaven og skal have hacker-looket: værter som glødende noder, firewall-glød i ejerens farve, krypteringslag i stedet for ringe og datalinjer i stedet for baner.
- Brugerfladen bliver terminalpaneler i Bevy UI.

## Turstruktur

Spillet kører med samtidige ture, ligesom Diplomacy og Neptune's Pride:

1. Alle crews planlægger ordrer for den næste tur samtidigt. Det er "dagskiftet".
2. Turen afvikles, når alle har afleveret, eller når fristen udløber. Det er "natskiftet".
3. Serveren simulerer turens dage, og alle får en log over, hvad der skete.

Med skiftevise ture ville et asynkront spil med flere spillere gå alt for langsomt. Originalen er allerede dagbaseret, så en tur bliver blot "N dage med automatik".

**Ordrer er stående ordrer.** Det er produktionskøer, forskningsvalg, exfil-ruter, flyveplaner og forsvarsholdning. Originalens automatik (ACC, AOC og MTX, her exfil-scripts, build-bots og krypterede links) passer godt til det. Engangshandlinger, som at bygge et citadel eller sende en orm af sted, udføres på turens første dag.

**Samtidige konflikter løses deterministisk.** Hvis to crews for eksempel bygger bagdør på den samme vært samme dag, afgør spillets seed og en prioritet, der roterer fra tur til tur.

## Konkurrence

- **Crews.** Hver crew har sit eget skjulested med egen forskning, træning, værksted og lager. Skjulestedet kan ikke indtages, så ingen crew kan blive slået helt ud.
- **Værter.** Den crew, der først bygger en bagdør på en vært, ejer den, og udvindingen der går til ejeren.
- **Heat.** Citadeller, taps og angreb efterlader spor. The Legacy Net går efter crewen med mest heat, så den førende bliver jaget.
- **The Legacy Net.** Hver crew har sin egen krigstilstand med den, som i originalen: krigen starter ved 6 citadeller eller efter for meget handel.
- **Crew mod crew.** En orm med daemons, en C2-controller og en operatør sendes mod en rivals vært. Kampen bruger samme regel som mod The Legacy Net: styrke = daemons × (operatørniveau + 4). Før afsendelsen vælges, hvad en sejr skal give: exfiltrér (stjæl fra lageret), plant tap (en del af udvindingen) eller overtag (citadellet skifter ejer). Forsvaret er automatisk med de daemons, der ligger i citadellet. Hvert angreb giver angriberen heat. Angriber to crews hinanden samme dag, kæmpes begge kampe i en rækkefølge, som seed'et bestemmer.
- **Beskyttelse.** I de første 5 ture kan crews ikke angribe hinanden.

## Portering af reglerne

Kortlagt i Godot-koden. Selve reglerne fylder ca. 2.000 linjer, og datatabellerne ca. 3.900 linjer i `CoreData.cs`.

| System | Godot-kilde | Rust | Status |
|---|---|---|---|
| Tilfældigheder | `Random.Shared` (uden seed) | `rng.rs` | ✅ PCG32 med seed |
| Personale og niveauer | `Objects/Staff.cs` | `staff.rs` | ✅ |
| Forskning | `Platform/Screens/Research.cs` | `research.rs` | ✅ |
| Datatabeller: genstande, netværk, værter | `CoreData.cs` | `data/` + `tools/extract_coredata.py` | M1 |
| Udvinding med taps og rekognoscering | `Objects/Planet.cs` | | M1 |
| Rekruttering | `Objects/Training.cs` | | M1 |
| Værksted og build-bots | `Factory.cs`, `Production.cs` | | M1 |
| Droppere, orme og tunnelskibe | `Ship.cs`, `InterStellarShip.cs`, `ShipInterior.cs` | | M1 |
| Exfil-scripts og krypterede links | `Objects/ACC.cs`, `MTX.cs` | | M1 |
| The Legacy Net og kamp | `EnemyFleets.cs`, `EnemyDroneBuilder.cs`, `BattleLogic.cs` | | M4 |
| Unlocks og beskeder | `Platform/Unlocker.cs` | | M1 |

**Fejl i Godot-koden, som ikke skal kopieres:**

- MTX-balancering skaber ressourcer ud af ingenting: formlen `(a + b/2)` i `MTX.cs:403`.
- `=` i stedet for `==` åbner forskning i star drive, hver gang skibshangaren åbnes (`ShipBay.cs:191`).
- Montering af en motor bruger ikke drevet (`Engine.cs:37-45`).
- `Next(Count - 1)` i opsætningen vælger aldrig den sidste planet (`CoreData.cs:68,4269,4283`).
- `Next(0,6)` giver aldrig silica eller de største asteroider, så kun 1 ud af 6 asteroider kan udvindes (`Asteroid.cs`).
- Planeter, der behandles i samme millisekund, får de samme tilfældige tal (`Planet.cs:61`).
- Et boretårn kan tage mere, end der er tilbage i åren (`Planet.cs:81`).
- Skibe fjernes fra en liste, mens den gennemløbes (`ShipInterior.cs:1091-1097`).

**Mangler i Godot-remaken, som skal designes:** gemte spil, sejr og nederlag, rejsetid mellem netværk og effekten af kvantelink, forstærker, honeypot, jammer, jæger-daemon, kill switch og logikbomber.

## Ophavsret

Godot-remakens kode er udgivet under CC0, så den må frit bruges. Dens grafik, lyd, skrifttyper og tekster stammer derimod fra originalspillet (© Activision 1991). NullNet bruger derfor procedurel grafik og egne tekster og genbruger ikke originalens materiale. Spilmekanik er ikke beskyttet af ophavsret, og navnet NullNet undgår originalens titel.

## Milepæle

| | Indhold |
|---|---|
| **M0** | Workspace, CI, designdokument, kerne med forskning og turafvikling, web-klient med procedurel oversigt. ✅ |
| **M1** | Kerneregler for én crew: datatabeller, udvinding, rekruttering, værksted, transport, exfil-scripts og krypterede links. Testes med et headless "bot-spil". |
| **M2** | Server: opret spil, invitationslinks, aflever ordrer, afvikl tur, gem i SQLite, fog of war. |
| **M3** | Klient i hacker-look: lobby, netværkskort, terminalpaneler for vært, citadel og transport, ordrepanel og turlog. |
| **M4** | The Legacy Net: AI, krig, kampafvikling med animeret genafspilning, erobring og befrielse. |
| **M5** | Crew mod crew: angreb, forsvar, heat, beskyttelse, sejrsbetingelser og balancering. |
| **M6** | Finish: effekter, lyd, notifikationer og hosting. |

## Åbne spørgsmål

- Standard for turlængde (spildage) og frist (timer)?
- Sejrsbetingelse og pointtabel.
- Navne på de 9 netværk og 160 værter.
- Skal man kunne logge ind på tværs af enheder (e-mail-login), eller er et invitationslink pr. spil nok?
- Hosting: Fly.io, en VPS eller noget tredje?
