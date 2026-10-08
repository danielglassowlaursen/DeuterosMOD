#!/usr/bin/env python3
"""Extracts the Deuteros rule tables from the Godot remake into NullNet data.

Reads Godot/Code/CoreData.cs, where the tables are written as C# object
initialisers, and writes crates/nullnet-core/data/classic.json with every
item renamed to its NullNet identifier (see docs/DESIGN.md). Only numbers
and structure are taken; the original's names and texts are not.

Usage: tools/extract_coredata.py [path/to/CoreData.cs] [output.json]
"""

import json
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
DEFAULT_SOURCE = ROOT / "Godot/Code/CoreData.cs"
DEFAULT_OUTPUT = ROOT / "crates/nullnet-core/data/classic.json"

# Deuteros star -> NullNet network. Each network is one corner of the net
# with its own naming theme, as each of Deuteros's star systems had.
NETWORKS = {
    "the_sun": "Metro",  # the city's net: exchange, utilities, transit, clinic
    "proxima": "Orbital",  # a satellite constellation and its ground stations
    "centauri": "Bankwire",  # financial mainframes
    "barnard": "Campus",  # a university's net
    "lalande": "Ministry",  # the state's net
    "sirius": "Nimbus",  # a pair of cloud regions
    "cygni": "Foundry",  # industrial control systems
    "procyon": "Helix",  # biotech laboratories
    "tau_ceti": "Lattice",  # the AI compute net where the Legacy Net was born
}

# Deuteros planet or moon -> NullNet host. Top-level hosts are servers,
# mainframes and facilities; their subsystems are the services on them.
HOSTS = {
    # Metro
    "mercury": "Beacon",
    "venus": "Switchboard",
    "earth": "Exchange",
    "the_moon": "Mirror",
    "mars": "Transit",
    "phobos": "Dispatch",
    "deimos": "Ticketing",
    "asteroids": "Scrapyard",
    "jupiter": "Colossus",
    "amalthea": "Payroll",
    "io": "Census",
    "europa": "Registry",
    "ganymede": "Permits",
    "callisto": "Courts",
    "leda": "Tax",
    "himalia": "Elections",
    "elara": "Records",
    "pasiphae": "Console",
    "saturn": "Waterworks",
    "mimas": "Pumps",
    "encaladus": "Valves",
    "tethys": "Filtration",
    "dione": "Reservoir",
    "rhea": "Telemetry",
    "titan": "Metering",
    "hyperion": "Sewer",
    "iapetus": "Floodgate",
    "phoebe": "Sampling",
    "uranus": "Powergrid",
    "miranda": "Switchgear",
    "ariel": "Turbine",
    "umbriel": "Breaker",
    "titania": "Substation",
    "oberon": "Control",
    "neptune": "Clinic",
    "triton": "Imaging",
    "neried": "Pharmacy",
    "nthree": "Triage",
    "nfour": "Bloodwork",
    "pluto": "Outpost",
    "charon": "Repeater",
    "decuria": "Lighthouse",
    # Orbital
    "atlantic": "Uplink",
    "pacific": "Constellation",
    "barent": "Polar",
    "baltic": "Relay",
    # Bankwire
    "chiron": "Clearinghouse",
    "cercops": "Vault",
    "circe": "Teller",
    "chimaera": "Escrow",
    "cerberus": "Ledger",
    "cronus": "Audit",
    "chloe": "Settlement",
    "calchas": "Forex",
    "cadmus": "Custody",
    "creon": "Mint",
    "cybele": "Press",
    "cupid": "Assay",
    # Campus
    "mycenae": "Registrar",
    "tyre": "Observatory",
    "ur": "Telescope",
    "thebes": "Library",
    "tanis": "Stacks",
    "memphis": "Catalogue",
    "karnak": "Manuscripts",
    "gizeh": "Microfilm",
    "calah": "Periodicals",
    "noria": "Maps",
    "abydos": "Theses",
    "saqqara": "Reading room",
    "pompeii": "Laboratory",
    "petra": "Centrifuge",
    "palmyra": "Cleanroom",
    "jericho": "Faculty",
    "babylon": "Physics",
    "troy": "Chemistry",
    "carthage": "Linguistics",
    "crete": "Supercomputer",
    "knossos": "Scheduler",
    "delphi": "Scratch",
    "ephesus": "Compiler",
    "corinth": "Solver",
    "athens": "Simulator",
    "olympia": "Visualiser",
    "mari": "Admissions",
    "cuzco": "Scholarships",
    # Ministry
    "nero": "Cabinet",
    "julius": "Intelligence",
    "septimus": "Wiretap",
    "augustus": "Dossiers",
    "claudius": "Ciphers",
    "hadrian": "Watchlist",
    # Nimbus
    "romulus": "Primary",
    "remus": "Replica",
    # Foundry
    "helios": "Solar",
    "lithos": "Quarry",
    "burah": "Kiln",
    "alumen": "Furnace",
    "silex": "Crucible",
    "sulfurum": "Refinery",
    "chloros": "Cracker",
    "argos": "Pipeline",
    "calx": "Scrubber",
    "titanes": "Smelter",
    "vanadis": "Blast",
    "chronos": "Ladle",
    "selene": "Caster",
    "bromos": "Mill",
    "kryptos": "Coke",
    "rubidos": "Slag",
    "zargun": "Assembly",
    "niobe": "Robotics",
    "kadmeia": "Welding",
    "tellus": "Paint",
    "iodes": "Conveyor",
    "xenos": "Inspection",
    "caesius": "Packaging",
    "rhenus": "Inventory",
    "osme": "Chemworks",
    "iris": "Mixer",
    "platina": "Distiller",
    "aurum": "Catalyst",
    "thallos": "Tankfarm",
    "astatos": "Flare",
    "radius": "Reactor",
    "aktis": "Core",
    "protos": "Coolant",
    "prasios": "Containment",
    # Helix
    "cambrian": "Genebank",
    "cainozoic": "Sequencer",
    "tertiary": "Primer",
    "paleocene": "Assembler",
    "eocene": "Aligner",
    "oligocene": "Annotator",
    "miocene": "Variant",
    "pliocene": "Phenotype",
    "paleozoic": "Cryostore",
    "silurian": "Freezer",
    # Lattice
    "alpha": "Cortex",
    "beta": "Tensor",
    "delta": "Shard",
    "gamma": "Oracle",
    "theta": "Inference",
    "iota": "Weights",
    "kappa": "Tokenizer",
    "epsilon": "Trainer",
    "lambda": "Epoch",
    "mu": "Gradient",
    "nu": "Batch",
    "xi": "Checkpoint",
    "omicron": "Optimizer",
    "pi": "Sampler",
    "zeta": "Hive",
    "rho": "Worker",
    "sigma": "Queue",
    "upsilon": "Broker",
    "phi": "Monitor",
    "chi": "Logger",
    "psi": "Replayer",
    "omega": "Sandbox",
}

# Deuteros item type -> NullNet ItemType (crates/nullnet-core/src/items.rs).
ITEMS = {
    "iron": "Compute",
    "titanium": "Storage",
    "aluminium": "Memory",
    "carbon": "Code",
    "copper": "Credentials",
    "hydrogen": "Bandwidth",
    "deuterium": "ExitNodes",
    "methane": "Proxies",
    "helium": "Keys",
    "paladium": "ZeroDays",
    "platinum": "Crypto",
    "silver": "Certificates",
    "gold": "SigningKeys",
    "silica": "Firmware",
    "meh_fuel": "ProxyChains",
    "hed_fuel": "OnionRoutes",
    "derrick": "Tap",
    "s_chassis": "DropperCore",
    "s_drive": "DropperEngine",
    "of_frame": "CitadelModule",
    "supply_pod": "DataContainer",
    "tool_pod": "ToolModule",
    "cryo_pod": "SessionPod",
    "i_chassis": "WormCore",
    "i_drive": "WormEngine",
    "a__c__c": "ExfilScript",
    "a__o__c": "BuildBot",
    "bandaid": "Patch",
    "s__d__m": "KillSwitch",
    "grapple": "Sniffer",
    "d__f__c__c": "C2Controller",
    "a__m__a": "Crawler",
    "hyperlight": "QuantumLink",
    "m__t__x": "EncryptedLink",
    "m__f__l": "Amplifier",
    "r_frame": "BackdoorKit",
    "prejudice_torpedo_launcher": "ExploitLauncher",
    "commspod": "ProtocolAdapter",
    "ios_drone": "Daemon",
    "g_chassis": "TunnelCore",
    "star_drive": "TunnelEngine",
    "p__t__l": "LogicBomb",
    "star_drone": "HunterDaemon",
    "prison_pod": "Honeypot",
    "sonic_blaster": "Jammer",
    "pulse_blaster_laser": "LegacyExploit",
    "alien_artifact": "SourceFragment",
}

# ResearchItem's constructor defaults (Godot/Code/Objects/ResearchItem.cs).
RESEARCH_DEFAULTS = {"multiplier": 64, "initial_value": 64, "limit": 100}


def item(name):
    try:
        return ITEMS[name]
    except KeyError:
        sys.exit(f"unknown Deuteros item type: {name}")


def boolean(text):
    return {"true": True, "false": False}[text]


def extract_items(src):
    items = []
    for match in re.finditer(r"var (\w+) = new Item\(\);", src):
        var = match.group(1)
        end = src.index(f"StaticGameData.ItemList.Add({var});", match.end())
        block = src[match.end() : end]

        def field(name, pattern=r"(\w+)"):
            found = re.search(rf"\b{var}\.{name} = {pattern};", block)
            return found.group(1) if found else None

        entry = {
            "item": item(field("ItemType", r"Enums\.ItemTypes\.(\w+)")),
            "category": {"item": "Item", "resource": "Resource"}[
                field("ItemCategory", r"Enums\.ItemCategory\.(\w+)")
            ],
            "mass": int(field("Mass", r"(\d+)")),
            "orbit_only": boolean(field("OrbitOnly") or "false"),
            "tool_module": boolean(field("ToolPod") or "false"),
            "tool_module_single": boolean(field("ToolPodSingular") or "false"),
            "auto_produce": boolean(field("AutoProduce") or "false"),
            "recipe": [
                [item(kind), int(count)]
                for kind, count in re.findall(
                    rf"\b{var}\.BuildRequirements\.Add\(new BuildRequirement\(Enums\.ItemTypes\.(\w+), (\d+)\)\);",
                    block,
                )
            ],
        }

        research = re.search(
            rf"\b{var}\.Research = new ResearchItem\(Enums\.ItemTypes\.\w+, (\d+), (\d+)\);", block
        )
        if research:
            overrides = dict(re.findall(rf"\b{var}\.Research\.(\w+) = (\w+);", block))
            entry["research"] = {
                "index": int(research.group(1)),
                "tech_level": int(research.group(2)),
                "multiplier": int(overrides.get("ResearchMultiplier", RESEARCH_DEFAULTS["multiplier"])),
                "initial_value": int(overrides.get("ResearchValue", RESEARCH_DEFAULTS["initial_value"])),
                "limit": int(overrides.get("ResearchLimit", RESEARCH_DEFAULTS["limit"])),
                # The constructor locks research; data unlocks what is open from the start.
                "available_at_start": not boolean(overrides.get("Locked", "true")),
                "researched_at_start": boolean(overrides.get("Researched", "false")),
            }
        items.append(entry)
    return items


def extract_networks(src):
    stars = re.findall(r"StaticGameData\.Stars\.Add\(Enums\.StellarBodies\.(\w+),", src)
    legacy_counts = dict(re.findall(r"starCounts\.Add\(StellarBodies\.(\w+), (\d+)\);", src))

    # Attack triggers are set in a switch over the star; consecutive cases share a value.
    triggers, pending = {}, []
    for case, value in re.findall(
        r"case Enums\.StellarBodies\.(\w+):|enemyFleet\.AttackTrigger = (\d+);", src
    ):
        if case:
            pending.append(case)
        else:
            triggers.update({star: int(value) for star in pending})
            pending = []

    return [
        {
            "classic": star,
            "name": NETWORKS[star],
            # Sol's Legacy hosts are fixed in the host table; elsewhere this many are picked at random.
            "random_legacy_hosts": int(legacy_counts[star]) if star in legacy_counts else None,
            "legacy_attack_trigger": triggers[star],
        }
        for star in stars
    ]


def extract_hosts(src, networks):
    network_index = {n["classic"]: i for i, n in enumerate(networks)}
    starts = list(
        re.finditer(
            r"StaticGameData\.Planets\.Add\(Enums\.StellarBodies\.(\w+), new Objects\.\w+\(Enums\.StellarBodies\.\w+, (\d+)\)",
            src,
        )
    )
    raw = []
    for i, match in enumerate(starts):
        end = starts[i + 1].start() if i + 1 < len(starts) else src.index("starCounts", match.end())
        block = src[match.end() : end]

        def field(name, pattern=r"(\w+)"):
            found = re.search(rf"\b{name} = {pattern},", block)
            return found.group(1) if found else None

        raw.append(
            {
                "classic": match.group(1),
                "name": HOSTS[match.group(1)],
                "order": int(match.group(2)),
                "network": network_index[field("ParentStar", r"Enums\.StellarBodies\.(\w+)")],
                "parent": field("MoonParentPlanetId", r"Enums\.StellarBodies\.(\w+)"),
                "resources": [
                    item(kind)
                    for kind in re.findall(r"new Objects\.Material\(Enums\.ItemTypes\.(\w+), \d+\)", block)
                ],
                "segment": boolean(field("Segment") or "false"),
                "legacy": boolean(field("ActiveMethanoid") or "false"),
                # The asteroid belt: a field of abandoned data caches, not a host to hold.
                "cache_field": match.group(1) == "asteroids",
            }
        )

    index = {host["classic"]: i for i, host in enumerate(raw)}
    for host in raw:
        host["parent"] = index[host["parent"]] if host["parent"] else None
    names = [host["name"] for host in raw]
    assert len(set(names)) == len(names), "host names must be unique"
    assert set(HOSTS) == set(index), "every host has a name and every name a host"
    return raw


def extract_rates(src, table):
    return {
        item(kind): int(value)
        for kind, value in re.findall(
            rf"StaticGameData\.{table}\[Enums\.ItemTypes\.(\w+)\] = (\d+);", src
        )
    }


def extract_training(src):
    fields = dict(re.findall(r"trainingData\.(\w+) = ([\d.]+);", src))
    kinds = {"Analyst": "Researcher", "Coder": "Production", "Operator": "Marines"}
    return {
        "recruits_available": int(fields["AvailableTrainees"]),
        "courses": {
            kind: {
                "days": int(fields[f"{prefix}TrainingTime"]),
                "batch_max": int(fields[f"{prefix}TrainingMax"]),
                "team_max": int(fields[f"{prefix}MaxCount"]) if f"{prefix}MaxCount" in fields else None,
            }
            for kind, prefix in kinds.items()
        },
    }


def main():
    source = Path(sys.argv[1]) if len(sys.argv) > 1 else DEFAULT_SOURCE
    output = Path(sys.argv[2]) if len(sys.argv) > 2 else DEFAULT_OUTPUT
    src = source.read_text(encoding="utf-8-sig")

    networks = extract_networks(src)
    hosts = extract_hosts(src, networks)
    home = next(i for i, h in enumerate(hosts) if h["classic"] == "earth")
    earth = src[src.index("Special Earth setup") :]

    data = {
        "source": "Godot/Code/CoreData.cs from DeuterosOrg/Deuteros-Resurrected, via tools/extract_coredata.py",
        "items": extract_items(src),
        "networks": networks,
        "hosts": hosts,
        "survey_multiplier": extract_rates(src, "ResourceLevels_Survey_Multiplier"),
        "tap_rate": extract_rates(src, "ResourceRate_Per_Derrick"),
        "hideout": {
            "host": home,
            "taps": int(re.search(r"\.Derricks = (\d+);", earth).group(1)),
        },
        "recruitment": extract_training(src),
    }

    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(data, indent=1, ensure_ascii=False) + "\n", encoding="utf-8")
    print(
        f"wrote {output.relative_to(ROOT) if output.is_relative_to(ROOT) else output}: "
        f"{len(data['items'])} items, {len(networks)} networks, {len(hosts)} hosts"
    )


if __name__ == "__main__":
    main()
