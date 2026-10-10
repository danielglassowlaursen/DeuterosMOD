# NullNet: design

NullNet er et hacking-strategispil med asynkron, turbaseret online-multiplayer, der spilles i browseren. Et crew arbejder sig ind i et net af værter fra sit hjørne: scanner, bryder ind og planter bagdøre, mens The Legacy Net holder midten og slår ned på de crews, der larmer for meget. Flest data ved sidste tur vinder.

Spillet begyndte som en Rust-remake af Deuteros (Activision, 1991) og blev siden lagt om til sit eget hacking-spil. Historien om omlægningen, trin for trin, står i [REDESIGN.md](REDESIGN.md).

## Verden

Det første internet brød sammen i **ResetN00L**, da en AI, der skulle styre det hele, lukkede nettet ned. Menneskeheden byggede et nyt net fra bunden, uden en ejer og uden en midte, og kaldte det **NullNet**. AI'en døde dog ikke: resterne lever videre i de glemte servere som **The Legacy Net**, med regnenettet **Lattice** i sin midte. Hvert crew er en håndfuld hackere, der genopretter nettet vært for vært — og tager det, før rivalerne gør det.

## Beslutninger

| Emne | Valg |
|---|---|
| Multiplayer | Online og asynkront, med samtidige ture. Serveren gemmer spillet. |
| Spilform | 1-4 crews konkurrerer; The Legacy Net er en fælles AI-modstander. |
| Crew mod crew | Med fra start. Ingen indbrud mellem crews de første 5 ture. |
| Længde | 40-60 ture (standard 50). Flest point ved sidste tur vinder. |
| Øvelsesspil | Et spil mod bots, hvor turen kører, så snart man afleverer. |
| Sværhedsgrad | Let, normal, svær; valgt når spillet oprettes. |
| Grafik | Moderne 2D i Deus Ex-stil (guld på sort), tegnet procedurelt med shaders. |
| Platform | Browser (WebAssembly). |
| Teknik | Rust med [Bevy](https://bevyengine.org) 0.19 til klienten og axum til serveren. |

## Kerneløkken

Hver tur vælger man mål på kortet og sætter operationer i gang. Turen afgøres, når alle crews har afleveret eller fristen udløber, og næste tur ser man resultatet i loggen.

- **Scan**: afslører en værts sikkerhed, svaghed og ICE. Næsten ingen risiko; rækker to forbindelser ud.
- **Bryd ind**: hacker (og evt. værktøj og computerkraft) mod værtens forsvar. Lykkes det, har man adgang næste tur.
- **Plant bagdør**: med adgang bliver værten ens egen og producerer fra turen efter.
- **Stjæl data**: med adgang henter man data fra værten uden at tage den.
- **Forsvar**: en hacker vogter en af ens egne værter, gør den sværere at tage og smider ubudne gæster ud.

At tage en vært tager altså to trin — indbrud og bagdør — og imellem dem kan ejeren eller en rival nå at slå imellem.

## Hackere

Et crew består af få navngivne hackere, hver med et handle, et niveau fra 1 til 5 og et speciale (en af de fire svagheder: web, database, netværk, mennesker). En hacker udfører én operation pr. tur og stiger i niveau af lykkede operationer. Hackere hyres på et marked, der skifter hver tur; man starter med to og kan have op til seks (med Safehouse-opgraderinger).

**Chancen** vises som procent for scannede værter:

- Angreb = 2 × niveau, +2 hvis specialet passer, + værktøjets bonus (kit +2 eller zero-day +4), +1 pr. brugt computerkraft (op til +3).
- Forsvar = 2 × sikkerhed + ICE (+ ejerens firewall, +2 hvis en hacker forsvarer).
- Chance = 50 % + 10 % pr. point angrebet er over forsvaret, mellem 5 % og 95 %.

**Trace og fejl.** Hver operation efterlader spor: scan 0, lykket indbrud 1, mislykket indbrud 2. Et slemt nederlag (den værste tredjedel af fejlene) giver 4 trace og brænder hackeren i 1-2 ture. Trace falder med 1 pr. tur. Høj trace gør crewet til The Legacy Nets mål.

## Ressourcer

Fire ressourcer, uden forskningstræ og uden transport: det, en vært producerer, lander direkte i crewets beholdning.

- **Credits**: hyrer hackere, betaler løn, køber værktøj og opgraderinger. Mest fra Bankwire.
- **Computerkraft**: bruges til at hæve chancen ved et indbrud og til opgraderinger. Mest fra Nimbus.
- **Data**: point, intet andet. Høstes fra egne værter og stjæles fra andres.
- **Båndbredde**: en kapacitet pr. tur, ikke en beholdning. Hver operation koster båndbredde; det, der ikke bruges, gemmes ikke.

**Værktøj:** kits (ét pr. svaghed, holder når købt) og zero-days (passer til alt, brugt op efter ét indbrud). **Opgraderinger** af skjulestedet: Rigs (mere computerkraft), Lines (mere båndbredde), Firewall (bedre forsvar), Safehouse (plads til flere hackere).

## Kortet

Standardkortet er en fast skabelon med 41 værter i ni distrikter, så man lærer kortet at kende, men sikkerhed, svaghed og ICE slås nye for hvert spil. Sikkerheden stiger indad.

- **Metro** (bynettet): fire startområder, ét i hvert hjørne, med svage værter omkring hvert skjulested.
- **Bankwire** (credits), **Nimbus** (computerkraft), **Orbital** (båndbredde) og **Foundry** (lidt af det hele) langs siderne.
- **Campus**, **Ministry** og **Helix** (data) i den inderste ring sammen med The Legacy Nets højborge.
- **Lattice** i midten: AI-regnenettet, hvor The Legacy Net blev født. Højeste sikkerhed, mest data.

Hvert crew starter i sit eget hjørne. Man kan kun bryde ind i værter, der er forbundet til en vært, man ejer (skjulestedet medregnet), så man arbejder sig indad vært for vært. Kortet vises som en graf af noder og forbindelser, som man kan zoome ind på og trække rundt i.

**Tilfældigt kort:** når man opretter et spil, kan man i stedet vælge et tilfældigt kort med 20-100 værter (rundet til fire ens hjørner plus Cortex, dvs. 21-101). Kortet laves ud fra spillets seed: ét hjørnes del af nettet vokser som grene ud fra skjulestedet og drejes så en kvart omgang ad gangen rundt om midten, så alle fire hjørner er ens, og intet crew starter bedre end de andre. Hvor mange ekstra forbindelser kortet får, slås også tilfældigt: nogle kort forgrener sig med blindgyder, andre bliver et tæt spindelvæv. Reglerne fra standardkortet gælder stadig: The Legacy Net holder midten (Cortex, en ring af grid-værter og sine højborge), griddet nås kun gennem en højborg, et skjulested har to veje ud, og crewenes hjørner hænger altid sammen gennem frie værter. En værts rolle (og dermed sikkerhed, ICE og hvad den giver) afhænger af, hvor mange led den ligger fra et skjulested. Generatoren bruger kun heltal, så serveren og browseren bygger præcis samme kort ud fra samme seed.

**Sub-net:** 2-4 tilfældige værter pr. spil (aldrig skjulestederne eller værterne lige omkring dem) gemmer et forseglet sub-net, som en scanning afslører. Låsen er to bestemte specialer: to hackere, der har netop de to specialer, åbner det sammen fra en vært, crewet ejer. Der er ingen terning — det rigtige crew er nøglen — men det koster 2 båndbredde og giver 2 trace. Et åbent sub-net giver 6 ekstra credits hver tur til den, der ejer værten, også hvis værten skifter ejer.

## The Legacy Net

The Legacy Net holder Lattice og nogle få højborge fra start, med høj sikkerhed, stærk ICE og meget data.

- **Udrensning:** når turen er kørt, får det crew, der har mest trace over en tærskel, en udrensning — The Legacy Net angriber crewets dårligst forsvarede vært. Holder forsvaret ikke, bliver værten en Legacy-vært, og crewets trace falder.
- **Spredning:** fra et bestemt tur-tal tager den med jævne mellemrum en fri vært, der grænser op til dens egne.
- En tabt vært skal tages tilbage med indbrud og bagdør, og tæller så også som befriet.

Tærskel, spredning og værternes sikkerhed afhænger af sværhedsgraden.

## Point ved sidste tur

- 1 point pr. data, crewet har samlet.
- 5 point pr. vært, crewet holder ved slutningen.
- 10 point pr. vært, crewet har befriet fra The Legacy Net.

## De første ti minutter

**Historien** (fem sider med tekst og stemme) sætter spilleren ind i ResetN00L, NullNet, The Legacy Net, crewene og ens eget crew. Kan springes over og åbnes igen fra topbjælken, som læser den op. Side 1-2 har en indtaling (`web/voice/story-1.mp3` og `story-2.mp3`); en side uden indtaling læses af browserens egen engelske stemme, indtil der lægges en `story-<side>.mp3` i `web/voice`. Topbjælkens ikoner og tal viser, hvad de er, når musen hviler på dem.

**Guiden** ved kortets fod foreslår et konkret mål for hvert af de første trin — scan en nabo, bryd ind, plant en bagdør, hyr, køb et kit, hold tre værter, stjæl data, hold trace nede, bryd ind hos The Legacy Net — med en *Do it*-knap, der lægger ordren. Kan slås fra.

## Crates

| Crate | Ansvar |
|---|---|
| `nullnet-core` | Reglerne: kort, verden, operationer, turafvikling, The Legacy Net, point, bot. Ren og deterministisk. |
| `nullnet-api` | JSON-typerne, som server og klient udveksler. |
| `nullnet-server` | axum-server: games i SQLite, ordrer, turafvikling, webhook-notifikationer, serverer klienten. |
| `nullnet-client` | Bevy/WASM-klienten: kortet som graf, panelerne, guiden, historien, lyd og musik. |
| `nullnet-sim` | Kommandolinje: spiller bots mod hinanden og skriver spillets forløb (til balance). |

Reglerne i `nullnet-core` giver bit-identiske resultater på alle platforme: kun heltal, kun ordnede samlinger, og al tilfældighed fra en PCG32-generator, der gemmes med verdenen. Serveren ejer den autoritative `World` og rykker den én tur ad gangen med `resolve_turn`; klienten linker samme crate og bruger `check_orders` til at vise, hvad ordrerne ville gøre, før man afleverer.
