# NullNet 2: hacking-spillet

Status: under design, trin for trin. Den nye regelkerne erstatter Deuteros-reglerne i `nullnet-core`. Server, ture, asynkron multiplayer, klient, UI, kort-tegning, historie, stemme, musik og guide genbruges.

## Hvorfor

Spillet er for svært at komme ind i, og regelkernen er en oversættelse af Deuteros: de 14 ressourcer og 35 genstande med opskrifter, forskningstræet, kurserne og holdenes niveauer, logistikken med dropper, citadel, worm, kajpladser, pods, brændstof, exfil-scripts og links, taps, årer og bagdøre, kortets opbygning efter Deuteros' stjernesystemer, planeter og måner, og The Legacy Nets krigsregel og cache-felterne. Den nye kerne bygges om hacking og er vores egen.

Vores egne og bliver: turstrukturen, serveren og API'et, klienten og UI'et, raids, heat og point som idéer, historien om ResetN00L, NullNet og The Legacy Net, guiden, stemmen og musikken.

## Trinene

1. Kerneløkken: hvad man gør hver tur. Besluttet.
2. Hacking: hvordan et indbrud foregår. Besluttet.
3. Økonomi: få, tydelige ressourcer, folk og værktøjer. I gang.
4. Kortet: vores eget netværk i stedet for Deuteros' stjernesystemer.
5. Modstanderne: The Legacy Net og de andre crews.
6. De første ti minutter: guide og historie til de nye regler.
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
