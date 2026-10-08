# Deuteros i Rust: design

Ny udgave af Deuteros (Activision, 1991) skrevet i Rust. Den skal have tidssvarende grafik og turbaseret online-multiplayer og spilles i browseren.

Udgangspunktet er Godot-remaken [DeuterosOrg/Deuteros-Resurrected](https://github.com/DeuterosOrg/Deuteros-Resurrected), som ligger i `Godot/`. Den bruges som opslagsværk for regler og data. Rust-udgaven bygges ved siden af den, ikke oven i den.

## Beslutninger

| Emne | Valg |
|---|---|
| Multiplayer | Online og asynkront. Hver spiller afgiver sin tur, når det passer, og serveren gemmer spillet. |
| Spilform | Spillerne konkurrerer mod hinanden, og Methanoids er en fælles AI-modstander. |
| Grafik | Moderne 2D. Planeter, stjerner og effekter tegnes procedurelt med shaders. |
| Platform | Browser (WebAssembly). |
| Sprog og motor | Rust med [Bevy](https://bevyengine.org) 0.19 til klienten og axum til serveren. |

## Arkitektur

```
crates/
  nullnet-core     Spillets regler. Ren Rust uden motor, ur eller I/O.
  deuteros-server   (M2) axum + SQLite. Autoritativ: gemmer spil og afvikler ture.
  nullnet-client   Bevy, kompileret til WebAssembly. Serveres af serveren.
Godot/              Godot-remaken: opslagsværk for regler, data og grafik.
```

### nullnet-core

- **Deterministisk.** Samme verden og samme ordrer giver altid samme resultat, på alle platforme. Derfor bruges kun heltal, kun ordnede samlinger (`BTreeMap`) og en egen PCG32-generator (`Rng`). Generatorens tilstand gemmes i verdenen, så en tur altid kan genafspilles. Den kommer ikke fra `rand`, fordi en opgradering af den pakke kunne ændre tallene.
- **To slags data.** `GameData` er de faste regeltabeller (genstande, forskning, planeter), og `World` er alt, der ændrer sig. Et gemt spil er en `World` serialiseret med serde.
- **Ordrer.** `Command` er en ordre til den kommende tur. En ugyldig ordre springes over og står i turrapporten. Den stopper aldrig turen.
- **Afvikling.** `resolve_turn(data, world, orders, days)` udfører ordrerne og kører derefter `days` dage. Hver dag kører systemerne i originalens rækkefølge: unlocks, minedrift, træning, produktion, skibe, forskning, Methanoid-droner, rumvæsen-beskeder og MTX.
- **Klienten bruger samme crate.** Så kan den vise, hvad en ordre vil gøre, før turen sendes. Resultatet bestemmes altid af serveren.

### deuteros-server (M2)

- Opretter spil og laver et invitationslink pr. spiller, med et hemmeligt token pr. spiller i linket.
- Modtager ordrer og afvikler turen, når alle har afleveret, eller når fristen udløber.
- Gemmer hver tur i SQLite: verdenen før turen, ordrerne og rapporten. Så kan man spole tilbage og finde fejl.
- Sender hver spiller kun det, spilleren kan se (fog of war). Modstandernes ordrer sendes aldrig.
- Giver besked, når en ny tur er klar. Første version gør det i browseren, senere eventuelt også med e-mail eller push.
- Én binærfil, der også serverer web-klienten, så den er let at hoste.

### nullnet-client

- Bevy 0.19 i browseren via WebGL2. Bygges med `scripts/build-web.sh`.
- Al grafik genereres i WGSL-shaders: planeter (sten, gas, jordlignende, is), ringe, atmosfære, sol og stjernefelt med parallakse. Klienten har ingen billedfiler.
- Brugerfladen laves med Bevy UI og et eget tema.

## Turstruktur

Spillet kører med samtidige ture (WEGO), ligesom Diplomacy og Neptune's Pride:

1. Alle spillere planlægger ordrer for den næste tur samtidigt.
2. Turen afvikles, når alle har afleveret, eller når fristen udløber (standard 24 timer).
3. Serveren simulerer turens dage, for eksempel 10 spildage, og alle får en rapport.

Med skiftevise ture ville et asynkront spil med flere spillere gå alt for langsomt. Originalen er allerede dagbaseret, så en tur bliver blot "N dage med automatik".

**Ordrer er stående ordrer.** Det er produktionskøer, forskningsvalg, ACC-fragtruter, flyveplaner og kampholdning. Originalens automatik (ACC, AOC, MTX) passer godt til det. Engangshandlinger, som at bygge en station eller sende et skib af sted, udføres på turens første dag.

**Kampe afvikles automatisk** ud fra spillerens holdning. Klienten viser dem bagefter som en animeret genafspilning.

**Samtidige konflikter løses deterministisk.** Hvis to spillere for eksempel bygger base på den samme planet samme dag, bruges spillets seed og en prioritet, der roterer fra tur til tur.

## Konkurrence-design (forslag)

Originalen er et spil for én spiller. Følgende er et forslag, der skal afprøves:

- **Fraktioner.** Hver spiller er en rumfartsorganisation med sin egen base på Jorden og sin egen forskning, træning, fabrikker og lager.
- **Planeter.** Den, der først bygger base eller station på en planet, ejer den. Minedrift på en planet tilhører ejeren.
- **Methanoids.** Hver spiller har sin egen krigstilstand med dem, så krig erklæres pr. spiller, som i originalen: ved 6 stationer eller efter handel. De angriber helst den førende spiller, hvilket giver de bagerste en chance for at indhente.
- **Spiller mod spiller.** Første version har ingen direkte kamp mellem spillere, kun kapløb om planeter, ressourcer og artefakter. Kamp mellem spillere kan komme senere.
- **Sejr.** Pointbaseret, ved en aftalt slutdag eller når en spiller når en pointgrænse. Der gives point for ejede planeter og stationer, befriede Methanoid-planeter, fundne artefakter (ét pr. stjerne, ni i alt) og forskning.

## Portering af reglerne

Kortlagt i Godot-koden. Selve reglerne fylder ca. 2.000 linjer, og datatabellerne ca. 3.900 linjer i `CoreData.cs`.

| System | Godot-kilde | Rust | Status |
|---|---|---|---|
| Tilfældigheder | `Random.Shared` (uden seed) | `rng.rs` | ✅ PCG32 med seed |
| Personale og niveauer | `Objects/Staff.cs` | `staff.rs` | ✅ |
| Forskning | `Platform/Screens/Research.cs` | `research.rs` | ✅ |
| Datatabeller: genstande, planeter, stjerner | `CoreData.cs` | `data/` + konverter | M1 |
| Minedrift og overvågning | `Objects/Planet.cs` | | M1 |
| Træning | `Objects/Training.cs` | | M1 |
| Produktion og AOC | `Factory.cs`, `Production.cs` | | M1 |
| Skibe, rejsetid og brændstof | `Ship.cs`, `InterStellarShip.cs`, `ShipInterior.cs` | | M1 |
| ACC og MTX | `Objects/ACC.cs`, `MTX.cs` | | M1 |
| Methanoid-AI og kamp | `EnemyFleets.cs`, `EnemyDroneBuilder.cs`, `BattleLogic.cs` | | M4 |
| Unlocks og bulletiner | `Platform/Unlocker.cs` | | M1 |

**Fejl i Godot-koden, som ikke skal kopieres:**

- MTX-balancering skaber ressourcer ud af ingenting: formlen `(a + b/2)` i `MTX.cs:403`.
- `=` i stedet for `==` åbner forskning i star drive, hver gang skibshangaren åbnes (`ShipBay.cs:191`).
- Montering af en motor bruger ikke drevet (`Engine.cs:37-45`).
- `Next(Count - 1)` i opsætningen vælger aldrig den sidste planet (`CoreData.cs:68,4269,4283`).
- `Next(0,6)` giver aldrig silica eller de største asteroider, så kun 1 ud af 6 asteroider kan udvindes (`Asteroid.cs`).
- Planeter, der behandles i samme millisekund, får de samme tilfældige tal (`Planet.cs:61`).
- Skibe fjernes fra en liste, mens den gennemløbes (`ShipInterior.cs:1091-1097`).

**Mangler i Godot-remaken, som skal designes:** gemte spil, sejr og nederlag, interstellar rejsetid og effekten af hyperlight, MFL, prison pod, sonic blaster, star drone, SDM og torpedoer.

## Ophavsret

Godot-remakens kode er udgivet under CC0, så den må frit bruges. Dens grafik, lyd, skrifttyper og tekster stammer derimod fra originalspillet (© Activision 1991). Rust-udgaven bruger derfor procedurel grafik og egne tekster og genbruger ikke originalens materiale. Spilmekanik er ikke beskyttet af ophavsret.

## Milepæle

| | Indhold |
|---|---|
| **M0** | Workspace, CI, designdokument, kerne med forskning og turafvikling, web-klient med procedurelt solsystem. ✅ |
| **M1** | Kerneregler for én spiller: datatabeller, minedrift, træning, produktion, skibe, ACC og MTX. Testes med et headless "bot-spil". |
| **M2** | Server: opret spil, invitationslinks, aflever ordrer, afvikl tur, gem i SQLite, fog of war. |
| **M3** | Klient: lobby, oversigt over solsystemet, skærme for planet, station og flåde, ordrepanel og turrapport. |
| **M4** | Methanoids: AI, krig, kampafvikling med animeret genafspilning, erobring og befrielse. |
| **M5** | Konkurrence: ejerskab, konflikter, sejrsbetingelser og balancering. |
| **M6** | Finish: lys, partikler, lyd, notifikationer og hosting. |

## Åbne spørgsmål

- Hvor mange spillere kan være i ét spil (2-8)?
- Standard for turlængde (spildage) og frist (timer)?
- Skal der være kamp mellem spillere, eller kun kapløb?
- Endelig sejrsbetingelse og pointtabel.
- Skal man kunne logge ind på tværs af enheder (e-mail-login), eller er et invitationslink pr. spil nok?
- Hosting: Fly.io, en VPS eller noget tredje?
