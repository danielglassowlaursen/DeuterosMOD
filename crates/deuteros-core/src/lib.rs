//! Deuteros game rules: a pure, deterministic simulation with no engine,
//! clock or I/O.
//!
//! The server owns the authoritative [`World`] and advances it one turn at a
//! time with [`resolve_turn`]; clients link the same crate to preview what
//! their orders will do. Everything here must give bit-identical results on
//! every platform, so the rules use integer arithmetic only, iterate ordered
//! collections only, and draw randomness from [`Rng`] stored in the world.

pub mod command;
pub mod ids;
pub mod items;
pub mod research;
pub mod rng;
pub mod staff;
pub mod turn;
pub mod world;

pub use command::{Command, CommandError};
pub use ids::{Day, PlayerId, calendar_date};
pub use items::ItemType;
pub use rng::Rng;
pub use turn::{Event, Orders, TurnReport, resolve_turn};
pub use world::{GameData, Player, World};
