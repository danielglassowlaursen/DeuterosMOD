use serde::{Deserialize, Serialize};

/// Every resource and manufactured item, in the order of the original
/// `Enums.ItemTypes` (Godot/Code/Enums.cs). The Deuteros name of each is
/// noted beside it; docs/DESIGN.md has the full glossary.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum ItemType {
    // Resources, extracted by taps.
    Compute,      // iron
    Storage,      // titanium
    Memory,       // aluminium
    Code,         // carbon
    Credentials,  // copper
    Bandwidth,    // hydrogen
    ExitNodes,    // deuterium
    Proxies,      // methane
    Keys,         // helium
    ZeroDays,     // palladium
    Crypto,       // platinum
    Certificates, // silver
    SigningKeys,  // gold
    Firmware,     // silica
    // Transit fuel, refined automatically in every workshop.
    ProxyChains, // MeH fuel: bandwidth + proxies
    OnionRoutes, // HeD fuel: keys + exit nodes
    // Manufactured items.
    Tap,             // derrick
    DropperCore,     // S chassis
    DropperEngine,   // S drive
    CitadelModule,   // OF frame
    DataContainer,   // supply pod
    ToolModule,      // tool pod
    SessionPod,      // cryo pod
    WormCore,        // I chassis
    WormEngine,      // I drive
    ExfilScript,     // ACC
    BuildBot,        // AOC
    Patch,           // bandaid
    KillSwitch,      // SDM
    Sniffer,         // grapple
    C2Controller,    // DFCC
    Crawler,         // AMA
    QuantumLink,     // hyperlight
    EncryptedLink,   // MTX
    Amplifier,       // MFL
    BackdoorKit,     // R frame
    ExploitLauncher, // prejudice torpedo launcher
    ProtocolAdapter, // comms pod
    Daemon,          // IOS drone
    TunnelCore,      // G chassis
    TunnelEngine,    // star drive
    LogicBomb,       // PTL
    HunterDaemon,    // star drone
    Honeypot,        // prison pod
    Jammer,          // sonic blaster
    LegacyExploit,   // pulse blaster laser
    SourceFragment,  // alien artifact
}
