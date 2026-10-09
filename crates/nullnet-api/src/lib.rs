//! The JSON the NullNet server and client exchange. Pure data, shared so
//! the client never has to pull the server in.
//!
//! Every crew is identified by the secret token in its invite link; its
//! routes hang under `/api/crew/{token}`.

use nullnet_core::{Command, CrewView, Day, PlayerId, TurnReport};
use serde::{Deserialize, Serialize};

/// `POST /api/games`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGame {
    pub name: String,
    pub crews: Vec<NewCrew>,
    /// Days each turn runs. Default 10.
    #[serde(default)]
    pub turn_days: Option<u32>,
    /// Hours a turn waits for orders before it runs anyway. Default 24.
    #[serde(default)]
    pub deadline_hours: Option<u32>,
    /// The game's last day; the highest score wins then if nobody has won
    /// before. Default 3000; 0 for no limit.
    #[serde(default)]
    pub end_day: Option<Day>,
    /// The map's seed; random if left out.
    #[serde(default)]
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NewCrew {
    pub name: String,
    /// Played by the server's bot instead of a person.
    #[serde(default)]
    pub bot: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    pub turn_days: u32,
    pub deadline_hours: u32,
    /// The game's last day, if it has one.
    #[serde(default)]
    pub end_day: Option<Day>,
}

/// The answer to `POST /api/games`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameCreated {
    #[serde(flatten)]
    pub game: GameInfo,
    pub crews: Vec<CrewInvite>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewInvite {
    pub player: PlayerId,
    pub name: String,
    pub bot: bool,
    /// The secret that identifies the crew; `None` for bots.
    pub token: Option<String>,
    /// The path of the crew's invite link on this server.
    pub join_path: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewState {
    pub player: PlayerId,
    pub name: String,
    pub bot: bool,
    pub submitted: bool,
}

/// `GET /api/crew/{token}`: the crew's own window on its game.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewStatus {
    pub game: GameInfo,
    pub player: PlayerId,
    pub name: String,
    pub turn: u32,
    pub day: u32,
    /// The server's clock when it answered, Unix seconds.
    pub now: i64,
    /// Unix time the running turn resolves at the latest.
    pub deadline: i64,
    pub submitted: bool,
    /// The crew's own orders for the running turn.
    pub orders: Vec<Command>,
    pub crews: Vec<CrewState>,
    pub view: CrewView,
    /// The last resolved turn, as this crew may see it.
    pub last_turn: Option<CrewTurn>,
    /// Whether the game is over; `view.ended` says how.
    #[serde(default)]
    pub over: bool,
}

/// `GET /api/crew/{token}/turns/{turn}`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewTurn {
    pub turn: u32,
    pub orders: Vec<Command>,
    pub report: TurnReport,
    pub resolved_at: i64,
}

/// `GET /api/crew/{token}/turns`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnSummary {
    pub turn: u32,
    pub first_day: u32,
    pub last_day: u32,
    pub resolved_at: i64,
}

/// The answer to `PUT /api/crew/{token}/orders`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrdersReceipt {
    /// Orders the rules would refuse as things stand, by position.
    pub rejected: Vec<RejectedOrder>,
    /// Whether this hand-in completed the turn and it ran.
    pub resolved: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RejectedOrder {
    pub index: usize,
    pub error: String,
}

/// The body of every error answer.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiError {
    pub error: String,
}

/// The crew's routes.
pub fn crew_path(token: &str) -> String {
    format!("/api/crew/{token}")
}
