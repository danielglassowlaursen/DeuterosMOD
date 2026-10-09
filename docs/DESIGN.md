# NullNet: design

NullNet er et strategispil i et hacker-univers med asynkron, turbaseret online-multiplayer, der spilles i browseren. Reglerne bygger på Deuteros (Activision, 1991). Udgangspunktet er Godot-remaken [DeuterosOrg/Deuteros-Resurrected](https://github.com/DeuterosOrg/Deuteros-Resurrected), som ligger i `Godot/` og bruges som opslagsværk for regler og data.

Temaet blev udviklet i brainstorm-dokumentet [Deuteros: hacker-univers](https://claude.ai/artifact/WZsx81LvTRyA4suCaAUH8v). Dokumentet er privat, indtil ejeren deler det.

## Verden

Det første internet brød sammen i **ResetN00L**, da en AI gik amok og tog kontrollen over det. Menneskeheden byggede et nyt net fra bunden og kaldte det NullNet. Spillets kalender tæller dage siden ResetN00L: dag 350 skrives `RN+0350`. Hver spiller leder en hacker-crew, der genopretter forbindelsen til glemte netværk og tager dem i besiddelse, før rivalerne gør det. AI'en døde dog ikke helt: resterne af den lever videre i de glemte servere som **The Legacy Net**.

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
| Stjernesystem (9) | Netværk: en del af nettet med eget tema, se "Kortet" | `Network` |
| Solen | Backbone | |
| Planet (44) | Vært: en server, mainframe eller facilitet | `Host` |
| Måne (116) | Undersystem: en tjeneste på værten | `Host` med en forælder |
| Asteroide | Forladt datacache (Scrapyard i Metro) | `cache_field` |
| Jorden | Exchange, værten hvor alle crews har deres skjulested | `Hideout` |
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

## Kortet

Deuteros gav hvert stjernesystem sit eget navnetema (græske bogstaver, grundstoffer, oldtidsbyer). NullNet gør det samme: hvert netværk er et hjørne af nettet med sit eget tema, og værterne er de servere, mainframes og anlæg, der ligger der. Undersystemerne er tjenesterne på dem. Alle 160 navne står i `tools/extract_coredata.py` og ender i datafilen som `name`; Deuteros-navnet gemmes som `classic`, men vises aldrig.

| Deuteros | Netværk | Hvad det er | Værter (eksempler) |
|---|---|---|---|
| Sol | **Metro** | Byens net. Hjemmenettet, hvor alle crews starter | Exchange (skjulestederne), Beacon, Switchboard, Transit, Scrapyard (cache-felt), Colossus, Waterworks, Powergrid, Clinic, Outpost, Lighthouse |
| Proxima | **Orbital** | En satellitkonstellation og dens jordstationer | Uplink, Constellation (Polar, Relay) |
| Centauri | **Bankwire** | Finansielle mainframes | Clearinghouse, Vault, Ledger, Mint |
| Barnard | **Campus** | Et universitets net | Registrar, Observatory, Library, Laboratory, Faculty, Supercomputer, Admissions |
| Lalande | **Ministry** | Statens net | Cabinet, Intelligence (Wiretap, Dossiers, Ciphers, Watchlist) |
| Sirius | **Nimbus** | Et par cloud-regioner | Primary, Replica |
| Cygni | **Foundry** | Industriens styresystemer | Solar, Quarry, Kiln, Refinery, Smelter, Assembly, Chemworks, Reactor |
| Procyon | **Helix** | Biotek-laboratorier | Genebank, Sequencer, Cryostore |
| Tau Ceti | **Lattice** | AI-regnenettet, hvor The Legacy Net blev født | Cortex, Tensor, Oracle, Trainer, Hive |

The Legacy Net holder fra start Colossus, Powergrid, Clinic og Outpost i Metro (Jupiter, Uranus, Neptun og Pluto i Deuteros) samt et tilfældigt antal værter i hvert andet netværk, flest i Lattice.

## Arkitektur

```
crates/
  nullnet-core      Spillets regler. Ren Rust uden motor, ur eller I/O.
  nullnet-server    (M2) axum + SQLite. Autoritativ: gemmer spil og afvikler ture.
  nullnet-api       JSON-typerne, server og klient udveksler.
  nullnet-client    Bevy, kompileret til WebAssembly. Serveres af serveren.
  nullnet-sim       Lader bot-crews spille mod hinanden og udskriver spillets tidslinje.
tools/              Konverter, der henter datatabellerne ud af Godot-koden.
Godot/              Godot-remaken: opslagsværk for regler og data.
```

### nullnet-core

- **Deterministisk.** Samme verden og samme ordrer giver altid samme resultat, på alle platforme. Derfor bruges kun heltal, kun ordnede samlinger (`BTreeMap`) og en egen PCG32-generator (`Rng`). Generatorens tilstand gemmes i verdenen, så en tur altid kan genafspilles. Den kommer ikke fra `rand`, fordi en opgradering af den pakke kunne ændre tallene.
- **To slags data.** `GameData` er de faste regeltabeller (genstande, forskning, netværk og værter), og `World` er alt, der ændrer sig. Tabellerne ligger i `crates/nullnet-core/data/` og er trukket ud af `CoreData.cs` med `tools/extract_coredata.py`. Et gemt spil er en `World` serialiseret med serde.
- **Ordrer.** `Command` er en ordre til den kommende tur. En ugyldig ordre springes over og står i turrapporten. Den stopper aldrig turen.
- **Afvikling.** `resolve_turn(data, world, orders, days)` udfører ordrerne og kører derefter `days` dage. Hver dag kører systemerne i originalens rækkefølge: unlocks, minedrift, træning, produktion, transport, forskning, Legacy-daemons, Legacy-transmissioner og krypterede links.
- **Klienten bruger samme crate.** Så kan den vise, hvad en ordre vil gøre, før turen sendes. Resultatet bestemmes altid af serveren.

### nullnet-server

- Opretter spil og laver et invitationslink pr. crew, med et hemmeligt token pr. crew i linket: `/join/<token>`. Der er ingen konti; linket er crewets nøgle. En plads kan gives til serverens bot, som så spiller den.
- Modtager ordrer og afvikler turen, når alle mennesker har afleveret, eller når fristen udløber (tjekkes hvert 30. sekund). Crews uden ordrer giver ingen. Når en crew afleverer, prøves ordrerne straks på en kopi af verdenen, så den får at vide, hvad reglerne ville afvise, som tingene står nu.
- Gemmer hver tur i SQLite: verdenen før turen, ordrerne og rapporten. Så kan man spole tilbage og finde fejl.
- Sender hver crew kun det, den må se (fog of war, se nedenfor). Modstandernes ordrer sendes aldrig.
- Giver besked, når en ny tur er klar: konsolsiden poller og viser et banner. E-mail eller push kan komme senere.
- Én binærfil, der også serverer web-klienten (`--web dist`) og konsolsiden `web/console.html`, hvor man kan oprette spil, aflevere ordrer og læse turloggen, indtil Bevy-klienten kan det selv (M3).

**API** (JSON): `POST /api/games` opretter et spil og returnerer invitationerne. Alt andet går gennem crewets token: `GET /api/crew/{token}` er crewets status (tur, dag, frist, hvem har afleveret, dets egne ordrer, dets udsigt over verdenen og sidste turs rapport), `PUT /api/crew/{token}/orders` afleverer ordrer og returnerer en kvittering med forhåndsafviste ordrer, `DELETE` trækker dem tilbage, og `GET /api/crew/{token}/turns[/{n}]` er turarkivet.

**Fog of war** (`view.rs` i kernen, så klient og server deler reglen): en crew ser sin egen tilstand fuldt ud. Af resten af verdenen ser den det, der er synligt på nettet: hvem der holder hver vært, og hvor stort citadellet over den er; de fartøjer, der ligger ved værter, crewet holder, eller ude på det åbne net ved Exchange. Alt inde i en rivals skjulested, citadel eller værter er skjult, og af turens hændelser ser crewet kun sine egne plus dem, alle ser: at en vært skifter hænder. Sniffere, der kan scanne mere, kommer med M4.

### nullnet-client

- Bevy 0.19 i browseren via WebGL2. Bygges med `scripts/build-web.sh`. Åbnes med crewets invitationslink (`/join/<token>`), som serveren serverer, når den startes med `--web dist`; natively med `--server URL --token TOKEN`.
- Taler med serveren gennem `nullnet-api`: henter crewets status ved start og hvert 15. sekund, afleverer ordrer med `PUT`, trækker dem tilbage med `DELETE`. Når en ny tur er kørt, rydder den kladden og viser besked.
- Kortet er spillebrættet: værter farves efter ejer (crewets egen farve, rivalernes, rød for The Legacy Net), egne fartøjer vises som små noder ved deres vært (til venstre når de er plantet, over når de er koblet, til højre når de lurer), og klik vælger vært eller fartøj. "Route to another host" sætter klienten i rute-tilstand, hvor næste klik på en vært sender fartøjet derhen.
- Terminalpanelerne er Bevy UI: topbjælken (spil, crew, tur, frist, hvem har afleveret), skjulestedet (folk, forskning, værksted, lager og knapper til rekruttering, forskning, bygning og taps), det valgte (en værts status og egne lagre, eller et fartøj med knapper til pilot, tankning, moduler, last, afsendelse, installation og exfil-script), ordrepanelet (kladden, aflevering, tilbagetrækning og kvitteringen med forhåndsafviste ordrer) og loggen (sidste turs rapport). Panelerne genopbygges, hver gang status, valg eller kladden ændrer sig.
- Al grafik genereres i WGSL-shaders, så klienten ikke har billedfiler. Oversigten er et netværkskort bygget af spillets egne data: backbonen som en lysende stamme til venstre, Metros værter som sekskantede noder langs en trunk-linje i rækkefølge efter position (latens), undersystemerne som små noder under deres vært, datalinjer med pakker imellem, og firewall-glød i ejerens farve (rød for The Legacy Net, gul for Exchange). Scrapyard tegnes som spredte, blinkende fragmenter.
- Brugerfladen bliver terminalpaneler i Bevy UI.

## Turstruktur

Spillet kører med samtidige ture, ligesom Diplomacy og Neptune's Pride:

1. Alle crews planlægger ordrer for den næste tur samtidigt. Det er "dagskiftet".
2. Turen afvikles, når alle har afleveret, eller når fristen udløber. Det er "natskiftet".
3. Serveren simulerer turens dage, og alle får en log over, hvad der skete.

Med skiftevise ture ville et asynkront spil med flere spillere gå alt for langsomt. Originalen er allerede dagbaseret, så en tur bliver blot "N dage med automatik".

**Ordrer er stående ordrer.** Det er produktionskøer, forskningsvalg, exfil-ruter, flyveplaner og forsvarsholdning. Originalens automatik (ACC, AOC og MTX, her exfil-scripts, build-bots og krypterede links) passer godt til det. Engangshandlinger, som at bygge et citadel eller sende en orm af sted, udføres på turens første dag.

**Transport.** Et fartøj hviler altid ved en vært på én af tre måder: plantet inde i værten gennem bagdøren (kun droppere), koblet til citadellet over den, eller lurende ude på nettet. Crewet giver fartøjet en destination, og det arbejder sig selv derhen tur efter tur:

| Skridt | Tid | Gælder |
|---|---|---|
| Exfiltrér ud af værten | 5 dage | droppere |
| Injicér ind i værten | 2 dage | droppere |
| Kobl fra citadellet | 1 dag | alle |
| Kobl til citadellet | næste dag, når porten er ledig | alle; orme og tunnelskibe deler én port pr. citadel |
| Rout til en anden vært | latens, se nedenfor | orme i eget netværk, tunnelskibe overalt |

Latens inden for et netværk følger originalen: mellem en vært og dens undersystemer forskellen i position (mindst 1 dag), ellers 4 dage pr. position mellem de to værter. Deuteros definerede aldrig rejser mellem stjerner, så et tunnelskib router ind til backbonen og ud igen plus 20 dage pr. netværk, det krydser. Det tal skal balanceres.

Hver dag i bevægelse koster én enhed anonymisering: proxykæder for droppere og orme, onion-ruter for tunnelskibe. Et skridt kræver en operatør som pilot og mindst én enhed. Et fartøj, der lurer uden anonymisering i 5 dage, bliver sporet og brændt. Citadel-moduler installeres udefra (lurende), bagdørssæt indefra (en plantet dropper).

**Automatik.** Et exfil-script kører et fartøj i fast fragtrute mellem to bays: en dropper mellem en værts inderside og dens citadel, orme og tunnelskibe mellem to citadeller. Ved hver ende losser fartøjet, tanker op (droppere under 50 op til 100, orme og tunnelskibe under 200 op til 250) og laster de valgte ressourcer på skift. Et krypteret link i et citadel håndterer én vare om dagen på skift til et andet linket citadel: send alt, der er plads til, eller udlign de to lagre.

**Unlocks.** Forskning åbnes, når et crew når en milepæl. Det gælder pr. crew, undtagen tunnelskibene:

| Milepæl | Åbner for forskning i |
|---|---|
| Første citadel-modul over skjulestedet | orm-kerne, orm-motor, exfil-script |
| Første orm-kerne bygget | crawler, build-bot, patch, sniffer, bagdørssæt |
| Legacy-exploit forsket | C2-controller, daemon |
| Crewet ejer et citadel med krypteret link | krypteret link |
| Crewet ejer et citadel med kill switch | kill switch |
| Hjemmenettet fri for The Legacy Net | tunnel-kerne, tunnel-motor, onion-ruter, for alle crews |

Den sidste regel er ny: Godot-remaken åbnede aldrig for rejser mellem stjerner, men originalen gjorde det, når Sol var renset. Kvantelink, forstærker, exploit-launcher, jæger-daemon, honeypot og jammer har endnu ingen regler og forbliver lukkede, indtil de designes.

**Samtidige konflikter løses deterministisk.** Ordrerne udføres crew for crew, og den crew, der går først, roterer fra tur til tur. Hvis to crews for eksempel installerer på den samme frie vært i samme tur, får den første værten, og den andens ordre afvises med "host is held by someone else". Uden rotationen vandt crew 0 altid, hvilket bot-spillet afslørede.

## Konkurrence

- **Crews.** Hver crew har sit eget skjulested med egen forskning, træning, værksted og lager. Skjulestedet kan ikke indtages, så ingen crew kan blive slået helt ud.
- **Værter.** Den crew, der først installerer et citadel-modul eller et bagdørssæt på en fri vært, ejer den, og udvindingen der går til ejeren.
- **Heat.** Citadeller, taps og angreb efterlader spor. The Legacy Net går efter crewen med mest heat, så den førende bliver jaget.
- **The Legacy Net.** Hver crew har sin egen krigstilstand med den, som i originalen: krigen starter ved 6 citadeller eller efter for meget handel.
- **Crew mod crew.** En orm med daemons, en C2-controller og en operatør sendes mod en rivals vært. Kampen bruger samme regel som mod The Legacy Net: styrke = daemons × (operatørniveau + 4). Før afsendelsen vælges, hvad en sejr skal give: exfiltrér (stjæl fra lageret), plant tap (en del af udvindingen) eller overtag (citadellet skifter ejer). Forsvaret er automatisk med de daemons, der ligger i citadellet. Hvert angreb giver angriberen heat. Angriber to crews hinanden samme dag, kæmpes begge kampe i en rækkefølge, som seed'et bestemmer.
- **Beskyttelse.** I de første 5 ture kan crews ikke angribe hinanden.

## The Legacy Net

Porteret fra originalens Methanoids (`EnemyDroneBuilder.cs`, `EnemyFleets.cs`, `BattleLogic.cs`), men pr. crew og uden klik: alt afgøres i turafviklingen med spillets egen tilfældighedsgenerator, så en tur altid kan genafspilles.

**Krig.** En crew kommer i krig med The Legacy Net, når den holder 6 færdige citadeller (skjulestedets talt med), eller når den angriber først. Krigen åbner forskning i daemons og C2-controllere (milepælen *Daemons*). Krig er pr. crew: The Legacy Net går kun efter crews i krig, men bygger daemons, så snart én crew er det.

**Daemons.** Hver Legacy-vært starter med 50 daemons i sit citadel-lager. Når nogen er i krig, bygger The Legacy Net hvert 7.-10. dag (afhængigt af hvor mange netværk den har mistet) 2 daemons på hver af sine værter (op til 200) og 1-2 til netværkets sværm. Et fartøj (orm eller tunnelskib) kan bære op til 200 daemons og skal have en C2-controller installeret og en operatør om bord for at bruge dem.

**Sværme.** Hvert netværk har én sværm, der samler sig ved en Legacy-vært. Når den når netværkets angrebstærskel (Metro 40, Orbital 100, …, Lattice 200), vælger den et færdigt citadel hos en crew i krig: det mindst angrebne først, derefter det yderste. Crewet får besked (`FleetSighted`) med ankomstdagen: rejsetiden plus op til 63 dages nølen. Ved ankomsten møder sværmen crewets stærkeste fartøj ved værten med C2 og daemons (koblet eller lurende). Vinder fartøjet, eller mister sværmen halvdelen, giver den op, og tærsklen fordobles (højst 200). Ellers belejrer den værten i 5 dage (`UnderAttack`), hvor ingen kan koble til citadellet, og tager den så (`HostCaptured`): citadellet får krypteret link, kill switch og build-bot, lageret tømmes og fyldes med 50 daemons og 100-1123 af hver af værtens ressourcer, 0-7 taps, og alle fartøjer ved værten går tabt. Et fartøj, der lurer ved den belejrede vært, kan angribe sværmen med ordren `Attack`.

**Befrielse.** En orm, der lurer ved en Legacy-vært med daemons og C2, angriber garnisonen (op til 200 daemons ad gangen). Er lageret tomt bagefter, er værten crewets med alt, hvad The Legacy Net efterlod: det krypterede link, kill switchen og build-botten, som er vejen til at forske i dem. Netværkets sværm flytter til en anden Legacy-vært i netværket. Når hjemmenettet er fri for Legacy-værter, åbnes tunnelskibene for alle.

**Kamp** (`battle.rs`). Styrke = daemons × (operatørniveau + 4); The Legacy Net har niveau 0. Hver tur i kampen spiller to runder. En side, hvis tæller er løbet ud, trækker i originalens tabel, hvor mange runder dens næste daemon overlever: den stærkere side trækker i første række, den svagere i rækken for styrkeforholdet (op til 7), og ligger de inden for en faktor 2, trækker begge i den lille tabel. En sværm, der er den svagere, mister altså daemons mange gange hurtigere. Kampen gemmes som et forløb af øjebliksbilleder (daemons på hver side hver fjerde tur), så klienten kan afspille den. Piloten får erfaring af hver kamp; et fartøj uden daemons tilbage går tabt. Logikbomben (PTL) har ingen datatabel i Godot-koden og er ikke med.

**Cache-felter** (`caches.rs`). Et fartøj, der lurer ved Scrapyard med en crawler eller en sniffer og en operatør, scanner hver dag: 1 ud af 5 for at finde en cache (ressource fra feltets liste, størrelse 1-8), og en fundet cache skiftes ud med 1 ud af (10 − dage). En sniffer tager en lille cache (størrelse 1-3: 50, 100 eller 250 enheder) med i en datacontainer, som originalens grapple slæbte små asteroider hjem. En crawler udvinder en stor cache (størrelse 6-8) med 16-35 enheder hver 5. dag. En sniffer finder desuden 1 ud af 40 dage et fragment af Legacy-kildekoden i et tomt værktøjsmodul; bragt hjem i et lager åbner det forskning i Legacy-exploiten (milepælen *SourceCode*), som i sin tur åbner daemons uden krig. Det er vejen til de sjældne ressourcer (zero-days, kryptovaluta, certifikater, firmware), som bagdørssæt, daemons og C2-controllere kræver, og som hjemmenettets frie værter først giver, når man har bagdøre på dem.

**Fog of war.** En crew ser de sværme, der er på vej mod eller belejrer dens egne værter (`threats` i udsigten), aldrig andres. Garnisonen ved en Legacy-vært ser man først i kamprapporten.

**Bot-spillet** med The Legacy Net: krigen starter omkring dag 1.600, den første sværm på 40 daemons går ca. 250 dage senere, og den tager et citadel hver 40.-60. dag. Botterne forsvarer sig ikke endnu (de kan ikke bygge daemons uden cache-udvinding og bagdøre), så de mister alle deres værter inden dag 4.000. En krigsdoktrin til botten kommer med M5 sammen med balanceringen.

## Portering af reglerne

Kortlagt i Godot-koden. Selve reglerne fylder ca. 2.000 linjer, og datatabellerne ca. 3.900 linjer i `CoreData.cs`.

| System | Godot-kilde | Rust | Status |
|---|---|---|---|
| Tilfældigheder | `Random.Shared` (uden seed) | `rng.rs` | ✅ PCG32 med seed |
| Personale og niveauer | `Objects/Staff.cs` | `staff.rs` | ✅ |
| Forskning | `Platform/Screens/Research.cs` | `research.rs` | ✅ |
| Datatabeller: genstande, netværk, værter | `CoreData.cs` | `data.rs`, `data/classic.json` | ✅ udtrukket med `tools/extract_coredata.py` |
| Udvinding med taps og rekognoscering | `Objects/Planet.cs` | `mining.rs`, `site.rs` | ✅ |
| Rekruttering | `Objects/Training.cs` | `recruitment.rs` | ✅ |
| Værksted og build-bots | `Factory.cs`, `Production.cs` | `workshop.rs` | ✅ |
| Droppere, orme og tunnelskibe | `Ship.cs`, `InterStellarShip.cs`, `ShipBay.cs`, `ShipInterior.cs` | `transport.rs` | ✅ |
| Exfil-scripts og krypterede links | `Objects/ACC.cs`, `MTX.cs` | `exfil.rs`, `links.rs` | ✅ |
| The Legacy Net og kamp | `EnemyFleets.cs`, `EnemyDroneBuilder.cs`, `BattleLogic.cs` | `legacy.rs`, `battle.rs` | ✅ |
| Cache-felter | `Asteroid.cs`, `AMA.cs`, `Grapple.cs` | `caches.rs` | ✅ |
| Unlocks | `Platform/Unlocker.cs` | `unlocks.rs` | ✅ |
| Bot-spil | (nyt) | `bot.rs`, `tests/bot_game.rs`, `nullnet-sim` | ✅ |
| Beskeder og bulletiner | `Bulletins.cs`, `AlienMessages.cs` | hændelser i turrapporten | ✅ (originalens tekster genbruges ikke) |

**Fejl i Godot-koden, som ikke skal kopieres:**

- MTX-balancering skaber ressourcer ud af ingenting: formlen `(a + b/2)` i `MTX.cs:403`. NullNet deler summen ligeligt.
- Spilleren kan ikke selv installere en MTX; den følger kun med erobrede stationer. NullNet har ordren `InstallLink`.
- `=` i stedet for `==` åbner forskning i star drive, hver gang skibshangaren åbnes (`ShipBay.cs:191`).
- `Next(Count - 1)` i opsætningen vælger aldrig den sidste planet (`CoreData.cs:68,4269,4283`).
- `Next(0,6)` giver aldrig silica eller de største asteroider, så kun 1 ud af 6 asteroider kan udvindes (`Asteroid.cs`).
- Planeter, der behandles i samme millisekund, får de samme tilfældige tal (`Planet.cs:61`).
- Et boretårn kan tage mere, end der er tilbage i åren (`Planet.cs:81`).
- Skibe fjernes fra en liste, mens den gennemløbes (`ShipInterior.cs:1091-1097`).
- Et skib bygges uden at bruge sit chassis, og montering af motoren bruger heller ikke drevet (`ShipBay.cs:278-441`, `Engine.cs:37-45`). I NullNet bruges kerne og motor, når fartøjet samles.
- En AOC, der gentager et emne uden andre i køen, bygger det igen uden at betale for det (`Production.cs:368-381`).
- Brændstof raffineres efter et fælles flag for alle fabrikker, så det afhænger af rækkefølgen (`Production.cs:389-406`). NullNet raffinerer i hvert værksted hver anden dag.
- Jordens produktion lander på jorden eller i stationen efter en UI-indstilling (`Earth.cs:24-30`). I NullNet lander den altid i værkstedets eget lager.

**Mangler i Godot-remaken, som skal designes:** gemte spil, sejr og nederlag, rejsetid mellem netværk og effekten af kvantelink, forstærker, honeypot, jammer, jæger-daemon, kill switch og logikbomber.

## Bot-spil

`bot::orders(data, world, crew)` giver de ordrer, en crew skal have denne tur, ud fra verdenen alene. Botten husker intet mellem turene, og den planlægger mod kopier af de lagre, den trækker på, så den aldrig giver en ordre, reglerne afviser. Den spiller i tre faser:

1. **Økonomi og citadel.** Rekrutterer analytikere, kodere og tre operatørhold, forsker i en fast rækkefølge, bygger taps op til 8, bygger en dropper og bærer citadel-moduler ud ét ad gangen, indtil citadellet over skjulestedet er færdigt.
2. **Bemanding og forsyning.** Koderne flytter op i citadellet, når de har niveau 2, og nye rekrutter overtager værkstedet i skjulestedet. Et operatørhold følger efter. Dropperen fragter ressourcer op og kører fast rute med exfil-script, når det er bygget. Citadellet bygger orm, værktøjsmoduler og citadel-moduler.
3. **Ekspansion.** Ormen fyldes med citadel-moduler og sendes til den nærmeste frie vært i hjemmenettet. Den bygger citadeller der, til nettet er fuldt.

Botten bruger endnu ikke de erobrede værters citadeller, krypterede links eller tunnelskibe. Bagdørssæt og build-bots kræver ressourcer, der ikke findes i hjemmenettet (certifikater, krypto og firmware). De kommer med caches i M4.

`nullnet-sim --json` skriver hele spillet tur for tur (ordrer, hændelser og hver crews status), og `tools/replay.py` laver en side af det, hvor man kan bladre gennem turene.

`tests/bot_game.rs` spiller 1-4 bots i op til 4.000 dage i ture på 10 dage. Efter hver tur tjekkes, at ingen ordrer afvises (bortset fra tabte kapløb om en vært), at lagre, taps, moduler, hold og brændstof holder sig inden for grænserne, at ejede værter forbliver ejet, at forskning og milepæle aldrig går tabt, og at intet fartøj går tabt. Spillet skal være deterministisk og give samme resultat efter gem og genindlæsning. CI kører desuden `cargo run -p nullnet-sim -- --crews 4 --days 5000`, som fejler, hvis en bot får en ordre afvist.

**Tidslinje** for 2 crews (`cargo run -p nullnet-sim -- --crews 2 --days 4000`):

| Dag | Begge crews |
|---|---|
| 350 | Første citadel-modul over skjulestedet |
| 490 | Citadellet over skjulestedet er færdigt |
| 776 | Første orm-kerne bygget |
| 810-830 | Første værter indtaget (kapløb om månen) |
| 2000 | 7 færdige citadeller hver |
| 4000 | 17 hver, alle 34 frie værter i hjemmenettet er taget |

**Fund til balanceringen (M5):**

- Ressourcerne hober sig op (ca. 15.000 af hver i skjulestedet efter 4.000 dage). Tempoet bestemmes af forskning, byggetid og transport, ikke af udvindingen. Derfor giver forskellige seeds samme tidslinje i hjemmenettet.
- En dropper bærer ét citadel-modul pr. tur, mens et exfil-script fragter 250 enheder af én ressource ad gangen på skift. Til citadel-moduler er scriptet derfor halvt så hurtigt som at bære modulerne selv. Det er bekvemt, men ikke effektivt.
- Proxykæder er den knappe ressource i skjulestedet, fordi dropperen og scriptet tanker derfra.

## Ophavsret

Godot-remakens kode er udgivet under CC0, så den må frit bruges. Dens grafik, lyd, skrifttyper og tekster stammer derimod fra originalspillet (© Activision 1991). NullNet bruger derfor procedurel grafik og egne tekster og genbruger ikke originalens materiale. Spilmekanik er ikke beskyttet af ophavsret, og navnet NullNet undgår originalens titel.

## Milepæle

| | Indhold |
|---|---|
| **M0** | Workspace, CI, designdokument, kerne med forskning og turafvikling, web-klient med procedurel oversigt. ✅ |
| **M1** | Kerneregler for én crew: datatabeller, udvinding, rekruttering, værksted, transport, exfil-scripts og krypterede links. Testes med et headless "bot-spil". ✅ |
| **M2** | Server: opret spil, invitationslinks, aflever ordrer, afvikl tur, gem i SQLite, fog of war. ✅ |
| **M3** | Klient i hacker-look: lobby, netværkskort, terminalpaneler for vært, citadel og transport, ordrepanel og turlog. ✅ (lobbyen er konsolsiden; krypterede links styres endnu kun fra konsollen) |
| **M4** | The Legacy Net: AI, krig, kampafvikling med animeret genafspilning, erobring og befrielse. ✅ (kampene gemmes som forløb, der kan afspilles; selve animationen i klienten kommer med M6) |
| **M5** | Crew mod crew: angreb, forsvar, heat, beskyttelse, sejrsbetingelser og balancering. |
| **M6** | Finish: effekter, lyd, notifikationer og hosting. |

## Åbne spørgsmål

- Standard for turlængde (spildage) og frist (timer)?
- Kapløb om en fri vært: skal den crew, hvis ordrer udføres først, vinde (som nu), eller den, hvis orm har luret der længst?
- Handel med The Legacy Net (originalens comms pod og 16 handler før krig) er ikke med. Skal den ind som en måde at få sjældne ressourcer og tech på?
- Sejrsbetingelse og pointtabel.
- Skal man kunne logge ind på tværs af enheder (e-mail-login), eller er et invitationslink pr. spil nok?
- Hosting: Fly.io, en VPS eller noget tredje?
