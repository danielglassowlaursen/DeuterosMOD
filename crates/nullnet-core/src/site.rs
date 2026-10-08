use serde::{Deserialize, Serialize};

use crate::data::HostDef;
use crate::items::ItemType;
use crate::store::Store;

/// One party's foothold on a host: the backdoor into it, the taps on it,
/// what has been extracted, and the citadel above it. A planet's ground base
/// and space station in Deuteros.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    /// Backdoor kits installed; [`Site::BACKDOOR_PARTS`] makes it complete.
    pub backdoor_parts: u32,
    /// A damaged backdoor stops extraction until it is patched.
    pub backdoor_damaged: bool,
    pub taps: u32,
    pub veins: Vec<Vein>,
    /// Extracted resources waiting on the host.
    pub store: Store,
    pub citadel: Citadel,
}

impl Site {
    pub const BACKDOOR_PARTS: u32 = 2;
    pub const MAX_TAPS: u32 = 8;

    /// An untouched site on `host`: no backdoor, no taps, and every resource
    /// one day away from its first survey result, as the original starts.
    pub fn new(host: &HostDef) -> Site {
        Site {
            veins: host
                .resources
                .iter()
                .map(|&resource| Vein::new(resource))
                .collect(),
            ..Site::default()
        }
    }

    pub fn backdoor_complete(&self) -> bool {
        self.backdoor_parts >= Self::BACKDOOR_PARTS
    }
}

/// One resource deposit: the vein being tapped, or the survey looking for
/// the next one once it runs dry.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vein {
    pub resource: ItemType,
    pub amount: u32,
    /// Days left until the survey finds the next vein; 0 with an empty vein
    /// means a new survey starts on the next mining day.
    pub survey_days: u32,
}

impl Vein {
    pub fn new(resource: ItemType) -> Vein {
        Vein {
            resource,
            amount: 0,
            survey_days: 1,
        }
    }
}

/// The citadel above a host: a space station in Deuteros.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Citadel {
    /// Citadel modules installed; [`Citadel::MODULES`] makes it complete.
    pub modules: u32,
    /// With an encrypted link, taps deliver straight into the citadel.
    pub encrypted_link: bool,
    pub kill_switch: bool,
    /// A build-bot runs the citadel's workshop without coders.
    pub build_bot: bool,
    pub store: Store,
}

impl Citadel {
    pub const MODULES: u32 = 8;

    pub fn complete(&self) -> bool {
        self.modules >= Self::MODULES
    }
}
