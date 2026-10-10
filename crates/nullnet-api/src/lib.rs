//! The JSON the NullNet server and client exchange. Pure data, shared so
//! the client never has to pull the server in.
//!
//! Every crew is identified by the secret token in its invite link; its
//! routes hang under `/api/crew/{token}`.

use nullnet_core::{Command, CrewView, PlayerId, TurnReport};
use serde::{Deserialize, Serialize};

/// `POST /api/games`
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CreateGame {
    pub name: String,
    pub crews: Vec<NewCrew>,
    /// `easy`, `normal` or `hard`. Default `normal`.
    #[serde(default)]
    pub difficulty: Option<String>,
    /// The game's last turn; the highest score wins then. Default 50.
    #[serde(default)]
    pub last_turn: Option<u32>,
    /// Hours a turn waits for orders before it runs anyway. Default 24; 0
    /// runs each turn the moment every person has handed in (a practice
    /// game against bots then runs turn by turn at once).
    #[serde(default)]
    pub deadline_hours: Option<u32>,
    /// A webhook (Discord or Slack style) the server posts to when a turn
    /// has run and when the game ends.
    #[serde(default)]
    pub notify_url: Option<String>,
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
    pub difficulty: String,
    pub last_turn: u32,
    pub deadline_hours: u32,
    /// Whether the game posts to a webhook when turns run.
    #[serde(default)]
    pub notifies: bool,
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
    /// The turn now being played, from 1.
    pub turn: u32,
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
    /// Whether the game is over; `view` carries the final scores.
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
