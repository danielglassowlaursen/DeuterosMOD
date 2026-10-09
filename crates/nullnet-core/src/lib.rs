//! NullNet game rules (built on the rules of Deuteros, 1991): a pure,
//! deterministic simulation with no engine, clock or I/O.
//!
//! The server owns the authoritative [`World`] and advances it one turn at a
//! time with [`resolve_turn`]; clients link the same crate to preview what
//! their orders will do. Everything here must give bit-identical results on
//! every platform, so the rules use integer arithmetic only, iterate ordered
//! collections only, and draw randomness from [`Rng`] stored in the world.

pub mod battle;
pub mod bot;
pub mod caches;
pub mod command;
pub mod data;
pub mod exfil;
pub mod ids;
pub mod items;
pub mod legacy;
pub mod links;
pub mod mining;
pub mod recruitment;
pub mod research;
pub mod rng;
pub mod site;
pub mod staff;
pub mod store;
pub mod transport;
pub mod turn;
pub mod unlocks;
pub mod view;
pub mod workshop;
pub mod world;

pub use battle::{Outcome, Report as BattleReport, Side as BattleSide};
pub use caches::Cache;
pub use command::{Command, CommandError};
pub use data::{GameData, HostDef, ItemCategory, ItemDef, NetworkDef};
pub use exfil::{ExfilScript, Route};
pub use ids::{Day, EPOCH, HostId, NetworkId, PlayerId, date};
pub use items::ItemType;
pub use legacy::{Fleet, Legacy};
pub use links::LinkConfig;
pub use rng::Rng;
pub use site::{Citadel, STAFF_SLOTS, Site, Vein};
pub use staff::{Staff, StaffKind};
pub use store::Store;
pub use transport::{
    AbortReason, Berth, Cargo, Destination, Module, ModuleKind, Seat, Vessel, VesselId, VesselKind,
    VesselState,
};
pub use turn::{Event, Orders, TurnReport, resolve_turn};
pub use unlocks::Milestone;
pub use view::{CrewSummary, CrewView, HostView, Threat, crew_view};
pub use workshop::{AutoMode, Job, SiteRef, Workshop, WorkshopRef};
pub use world::{Controller, HostState, Player, World};
