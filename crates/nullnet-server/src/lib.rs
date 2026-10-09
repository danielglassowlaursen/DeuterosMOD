//! The NullNet game server: one binary that keeps games in SQLite, takes
//! orders from crews, runs turns when everyone has handed in or the
//! deadline passes, and serves the web client.

pub mod api;
pub mod db;
pub mod games;
pub mod notify;

pub use api::router;
pub use games::{Server, now};
