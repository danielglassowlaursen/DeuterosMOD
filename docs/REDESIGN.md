# NullNet 2: hacking-spillet

Status: under design, trin for trin. Den nye regelkerne erstatter Deuteros-reglerne i `nullnet-core`. Server, ture, asynkron multiplayer, klient, UI, kort-tegning, historie, stemme, musik og guide genbruges.

## Hvorfor

Spillet er for svært at komme ind i, og regelkernen er en oversættelse af Deuteros: de 14 ressourcer og 35 genstande med opskrifter, forskningstræet, kurserne og holdenes niveauer, logistikken med dropper, citadel, worm, kajpladser, pods, brændstof, exfil-scripts og links, taps, årer og bagdøre, kortets opbygning efter Deuteros' stjernesystemer, planeter og måner, og The Legacy Nets krigsregel og cache-felterne. Den nye kerne bygges om hacking og er vores egen.

Vores egne og bliver: turstrukturen, serveren og API'et, klienten og UI'et, raids, heat og point som idéer, historien om ResetN00L, NullNet og The Legacy Net, guiden, stemmen og musikken.

## Trinene

1. Kerneløkken: hvad man gør hver tur. Besluttet.
2. Hacking: hvordan et indbrud foregår. Besluttet.
3. Økonomi: få, tydelige ressourcer, folk og værktøjer. Besluttet.
4. Kortet: vores eget netværk i stedet for Deuteros' stjernesystemer. Besluttet.
5. Modstanderne: The Legacy Net og de andre crews. Besluttet.
6. De første ti minutter: guide og historie til de nye regler. I gang.
7. Bygningen: den nye kerne i Rust, trin for trin, med resten genbrugt.

## 1. Kerneløkken (besluttet)

- Hver crew starter med et skjulested og et lille hold hackere og kan angribe en svag vært i første tur.
- Hver tur vælger man mål på kortet og sætter operationer i gang: scanne, bryde ind, stjæle data, forsvare. Turen afgøres, når alle har afleveret eller fristen udløber, og næste tur ser man resultatet.
- Værter, man kontrollerer, producerer ressourcer hver tur og åbner vejen til værter længere inde i nettet. Intet skal fragtes.
- Ressourcerne bruges på bedre værktøjer, flere folk og forsvar af egne værter.
- The Legacy Net forsvarer de bedste værter og slår igen, når man bliver for synlig.
- Hacking afgøres med ordrer og terningslag, når turen kører. Et minispil i Deus Ex-stil kan lægges ovenpå senere som bonus.
- Et spil varer 40-60 ture.
- Flest point ved sidste tur vinder.
- Historien bliver. Teksterne tilpasses de nye regler, og stemmen genindtales.

## 2. Hacking (besluttet)

**Værter.** Hver vært har et sikkerhedsniveau fra 1 til 5, en svaghed og et forsvar (ICE). Svagheden er én af fire: web, database, netværk eller mennesker. Indtil man har scannet en vært, ser man kun dens navn og type.

**Hackere.** Et crew består af få navngivne hackere, hver med et handle, et niveau fra 1 til 5 og et speciale, der svarer til en af de fire svagheder. En hacker udfører én operation pr. tur og stiger i niveau af lykkede operationer.

**At tage en vært tager to trin.** Et lykket indbrud giver adgang. Næste tur kan man plante en bagdør, og så er værten ens egen og producerer fra turen efter. Mens man kun har adgang, kan ejeren eller en rival nå at slå imellem: ejeren kan rense værten og smide den ubudne gæst ud, og har to crews adgang samtidig, får den første bagdør værten.

**Operationerne** er ordrer, der afgøres, når turen kører:
- *Scan*: afslører sikkerhed, svaghed og ICE. Næsten ingen risiko.
- *Bryd ind*: hacker og eventuelt værktøj mod værtens forsvar. Lykkes det, har man adgang.
- *Plant bagdør*: kræver adgang; værten bliver ens egen.
- *Stjæl data*: kræver adgang; henter data fra værten uden at tage den.
- *Forsvar*: en hacker vogter en af ens egne værter, gør den sværere at tage og renser den for ubudne gæster.

**Chancen** vises som procent, før man afleverer, men kun for scannede værter; en uscannet vært viser "ukendt". Første bud, der skal balanceres:
- Angreb = 2 × hackerens niveau, +2 hvis specialet passer til svagheden, + værktøjets bonus, +1 pr. brugt computerkraft op til +3.
- Forsvar = 2 × sikkerhed + ICE, +2 hvis en hacker forsvarer.
- Chance = 50 % + 10 % pr. point angrebet er over forsvaret, mellem 5 % og 95 %.

**Trace og fejl.** Hver operation efterlader spor, som lægges til crewets trace og falder med 1 pr. tur: scanning 0, lykket indbrud 1, mislykket indbrud 2. Et slemt nederlag, den værste tredjedel af fejlslagene, giver 4 trace og brænder hackeren, så han eller hun er ude i 1-2 ture. Høj trace gør crewet til The Legacy Nets mål (trin 5).

## 3. Økonomi (besluttet)

**Fire ressourcer.** Ingen forskningstræ, ingen opskrifter og ingen transport: det, en vært producerer, lander direkte i crewets beholdning.
- *Credits*: crewets penge. Hyrer hackere, betaler løn, køber værktøj og opgraderinger. Kommer kun fra værter, mest fra banker og butikker.
- *Computerkraft*: bruges på en operation for at hæve chancen, op til +3, og på opgraderinger. Kommer fra servere og datacentre.
- *Data*: point, intet andet. Høstes fra egne værter og stjæles fra andres.
- *Båndbredde*: en kapacitet pr. tur, ikke en beholdning. Hver operation koster båndbredde, og det, der ikke bruges, gemmes ikke. Første bud: scan 1, bryd ind 2, plant bagdør 1, stjæl data 2, forsvar 1. Skjulestedet giver en grundkapacitet, og værter med netværk giver mere.

**Hackere** hyres på et marked, der skifter hver tur: handle, niveau, speciale og pris. Man starter med to og kan have op til seks. Hver hacker får en lille løn pr. tur efter niveau.

**Værktøjer** købes på det sorte marked og passer til en svaghed. Et kit (fx SQL-injection til databaser) holder, når det er købt. En zero-day passer til alle svagheder, giver mere og er brugt op efter ét indbrud.

**Opgraderinger** afløser forskningstræet: få forbedringer af skjulestedet, købt for credits og computerkraft, fx bedre rigs (mere computerkraft), firewall (bedre forsvar af egne værter), flere linjer (mere båndbredde) og plads til flere hackere.

## 4. Kortet (besluttet)

**En fast skabelon med nye værdier hvert spil.** Nettet og navnene er de samme hver gang, så man lærer kortet at kende, men sikkerhed, svaghed og ICE slås nye for hver vært, når et spil oprettes. Scanning betyder derfor noget hvert spil.

**Omkring 40 værter** i ni distrikter med vores egne navne og hvert sit præg:
- *Metro*: byens net. Fire startområder i hver sit hjørne, med svage værter omkring hvert skjulested.
- *Bankwire*: banker og betalinger. Mest credits.
- *Nimbus*: cloud og datacentre. Mest computerkraft.
- *Orbital*: satellitter og jordstationer. Mest båndbredde.
- *Campus*, *Ministry* og *Helix*: universitet, stat og biotek. Mest data.
- *Foundry*: industriens styresystemer. Lidt af det hele.
- *Lattice*: AI-regnenettet i midten, hvor The Legacy Net blev født. Højeste sikkerhed, mest data.

Sikkerheden stiger indad: værterne nær hjørnerne har 1-2, mellemringen 2-4 og Lattice 4-5.

**Hvert crew starter i sit eget hjørne** med skjulestedet og bygger sit område op; crewene mødes inde i nettet. Med to eller tre crews står de tomme hjørner som frie værter.

**Rækkevidde.** Man kan kun bryde ind i værter, der har en forbindelse til en vært, man ejer, skjulestedet medregnet. Man arbejder sig indad vært for vært, og kortet får fronter og flaskehalse. Første bud: scanning rækker to forbindelser ud, så man kan planlægge næste skridt.

**På skærmen** bliver kortet en graf af noder og forbindelser i stedet for den nuværende stamme med undersystemer. Kort-tegningen og dens shaders genbruges.

## 5. Modstanderne (besluttet)

**The Legacy Net** holder Lattice i midten og nogle få højborge fra start. Dens værter har høj sikkerhed, stærk ICE og meget data.
- *Den reagerer på trace.* Når turen er kørt, får det crew, der har mest trace over en tærskel, en udrensning: The Legacy Net angriber den af crewets værter, der er dårligst forsvaret. Holder forsvaret ikke, bliver værten en Legacy-vært, og crewets trace falder, så det ikke rammes hver tur. Første bud: tærskel 6 trace, udrensningens styrke 4 + 1 for hver femte tur, mod værtens forsvar, og trace falder med 3 efter en udrensning.
- *Den breder sig langsomt.* Første bud: fra tur 10 tager den hver fjerde tur en fri vært, der grænser op til dens egne. Den kæmper ikke om crewenes værter på den måde; det gør kun udrensningerne.
- *En tabt vært bliver en Legacy-vært.* Den skal tages tilbage med indbrud og bagdør, og så tæller den også som befriet.

**De andre crews.** En rivals vært brydes ind i som alle andre. Med adgang kan man stjæle data eller plante en bagdør og tage værten. Ejeren kan i mellemtiden forsvare og rense den. De første 5 ture kan crews ikke bryde ind hos hinanden; scanning er tilladt. Trace erstatter den nuværende heat.

**Point ved sidste tur.** Første bud, der skal balanceres:
- 1 point pr. data, som crewet har samlet.
- 5 point pr. vært, crewet holder ved slutningen.
- 10 point pr. vært, crewet har taget fra The Legacy Net, hvad enten den holdes til slut eller ej.
