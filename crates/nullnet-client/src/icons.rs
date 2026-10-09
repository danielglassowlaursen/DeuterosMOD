//! The icons, as glyphs in the Lucide icon font (ISC licence, see
//! `assets/fonts/Lucide-LICENSE.txt`). The font in `assets/fonts` holds only
//! the glyphs named here: after adding one, run `scripts/subset-icons.py`
//! with the full `lucide.ttf` from the `lucide-static` npm package.

use nullnet_core::{ItemType, StaffKind, VesselKind};

pub const CPU: char = '\u{E0A9}';
pub const HARD_DRIVE: char = '\u{E0ED}';
pub const MEMORY: char = '\u{E445}';
pub const CODE: char = '\u{E093}';
pub const KEY_ROUND: char = '\u{E4A3}';
pub const RADIO_TOWER: char = '\u{E404}';
pub const DOOR_OPEN: char = '\u{E3D6}';
pub const SHUFFLE: char = '\u{E15E}';
pub const KEY: char = '\u{E0FD}';
pub const BUG: char = '\u{E20C}';
pub const BITCOIN: char = '\u{E05B}';
pub const BADGE: char = '\u{E241}';
pub const SIGNATURE: char = '\u{E5F2}';
pub const CIRCUIT: char = '\u{E403}';
pub const FUEL: char = '\u{E2AF}';
pub const HOUSE: char = '\u{E0F5}';
pub const CASTLE: char = '\u{E3E0}';
pub const FLASK: char = '\u{E0D5}';
pub const WRENCH: char = '\u{E1B1}';
pub const HAMMER: char = '\u{E0EC}';
pub const USER_PLUS: char = '\u{E1A2}';
pub const USERS: char = '\u{E1A4}';
pub const HEADSET: char = '\u{E5BD}';
pub const TERMINAL: char = '\u{E181}';
pub const MICROSCOPE: char = '\u{E2E4}';
pub const PACKAGE: char = '\u{E129}';
pub const ROCKET: char = '\u{E286}';
pub const ORBIT: char = '\u{E3E7}';
pub const PLUG: char = '\u{E37F}';
pub const SERVER: char = '\u{E153}';
pub const BOT: char = '\u{E1BB}';
pub const SKULL: char = '\u{E221}';
pub const SWORDS: char = '\u{E2B4}';
pub const FLAME: char = '\u{E0D2}';
pub const TROPHY: char = '\u{E373}';
pub const HOURGLASS: char = '\u{E296}';
pub const SEND: char = '\u{E152}';
pub const UNDO: char = '\u{E2A1}';
pub const TRASH: char = '\u{E18E}';
pub const CLOSE: char = '\u{E1B2}';
pub const CHECK: char = '\u{E06C}';
pub const HELP: char = '\u{E082}';
pub const VOLUME: char = '\u{E1AB}';
pub const VOLUME_OFF: char = '\u{E1AC}';
pub const MUSIC: char = '\u{E122}';
pub const BELL: char = '\u{E059}';
pub const BOOK: char = '\u{E05F}';
pub const SCROLL: char = '\u{E45F}';
pub const LIST: char = '\u{E106}';
pub const TARGET: char = '\u{E180}';
pub const ALERT: char = '\u{E193}';
pub const SHIELD: char = '\u{E158}';
pub const CROWN: char = '\u{E1D6}';
pub const HEXAGON: char = '\u{E0F3}';
pub const CHEVRON: char = '\u{E06F}';
pub const RADAR: char = '\u{E497}';
pub const ROUTE: char = '\u{E53E}';

/// The icon for a resource or an item.
pub fn item(item: ItemType) -> char {
    match item {
        ItemType::Compute => CPU,
        ItemType::Storage => HARD_DRIVE,
        ItemType::Memory => MEMORY,
        ItemType::Code => CODE,
        ItemType::Credentials => KEY_ROUND,
        ItemType::Bandwidth => RADIO_TOWER,
        ItemType::ExitNodes => DOOR_OPEN,
        ItemType::Proxies => SHUFFLE,
        ItemType::Keys => KEY,
        ItemType::ZeroDays => BUG,
        ItemType::Crypto => BITCOIN,
        ItemType::Certificates => BADGE,
        ItemType::SigningKeys => SIGNATURE,
        ItemType::Firmware => CIRCUIT,
        ItemType::ProxyChains | ItemType::OnionRoutes => FUEL,
        ItemType::Tap => PLUG,
        ItemType::CitadelModule => CASTLE,
        ItemType::DropperCore | ItemType::DropperEngine => PACKAGE,
        ItemType::WormCore | ItemType::WormEngine => ROCKET,
        ItemType::TunnelCore | ItemType::TunnelEngine => ORBIT,
        ItemType::Daemon | ItemType::HunterDaemon => BOT,
        ItemType::C2Controller => RADAR,
        ItemType::ExfilScript => ROUTE,
        ItemType::SourceFragment | ItemType::LegacyExploit => SKULL,
        _ => HEXAGON,
    }
}

/// The icon for a kind of staff.
pub fn staff(kind: StaffKind) -> char {
    match kind {
        StaffKind::Analyst => MICROSCOPE,
        StaffKind::Coder => TERMINAL,
        StaffKind::Operator => HEADSET,
    }
}

/// The icon for a kind of vessel.
pub fn vessel(kind: VesselKind) -> char {
    match kind {
        VesselKind::Dropper => PACKAGE,
        VesselKind::Worm => ROCKET,
        VesselKind::Tunneler => ORBIT,
    }
}
