//! NullNet game rules: a pure, deterministic hacking game with no engine,
//! clock or I/O.
//!
//! Crews work a fixed map of hosts from the corners inward, scanning, breaking
//! in and planting backdoors, while the Legacy Net holds the middle and sweeps
//! the crews that make too much noise. The server owns the authoritative
//! [`World`] and advances it one turn at a time with [`resolve_turn`]; clients
//! link the same crate to preview their orders with [`check_orders`].
//!
//! Everything here gives bit-identical results on every platform: the rules
//! use integer arithmetic only, iterate ordered collections only, and draw
//! randomness from [`Rng`] stored in the world.

pub mod bot;
pub mod command;
pub mod data;
pub mod ids;
pub mod legacy;
pub mod rng;
pub mod score;
pub mod turn;
pub mod view;
pub mod world;

pub use bot::orders as bot_orders;
pub use command::{Command, CommandError, Operation};
pub use data::{District, GameData, HostDef, Role, Upgrade, Weakness, Yields, rules};
pub use ids::{EPOCH, HackerId, HostId, PlayerId};
pub use legacy::spread_front;
pub use rng::Rng;
pub use score::{GameEnd, Score, score, scores};
pub use turn::{
    Event, Orders, Outcome, RejectedCommand, TurnReport, attack, chance, check_orders, defence,
    resolve_turn,
};
pub use view::{CrewSummary, CrewView, HostView, Intel, crew_view};
pub use world::{
    Access, Controller, Crew, Difficulty, Hacker, HostState, Offer, Settings, Upgrades, World,
};
