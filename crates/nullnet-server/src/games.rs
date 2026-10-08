//! Running games: creating them, taking orders and resolving turns. The
//! rules are the core's; this is the clock, the mailbox and the archive
//! around them.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use nullnet_core::{
    Command, CrewView, GameData, Orders, PlayerId, TurnReport, World, bot, crew_view, resolve_turn,
};
use serde::{Deserialize, Serialize};

use crate::db::{CrewRow, GameRow, Store, TurnRow, TurnSummary};

/// Unix time in seconds.
pub fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub struct Server {
    data: GameData,
    db: Mutex<Store>,
}

#[derive(Debug)]
pub enum Error {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl From<rusqlite::Error> for Error {
    fn from(error: rusqlite::Error) -> Self {
        Error::Internal(error.to_string())
    }
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotFound(m) | Error::BadRequest(m) | Error::Internal(m) => f.write_str(m),
        }
    }
}

// ------------------------------------------------------------ requests

#[derive(Clone, Debug, Deserialize)]
pub struct CreateGame {
    pub name: String,
    pub crews: Vec<NewCrew>,
    /// Days each turn runs. Default 10.
    #[serde(default)]
    pub turn_days: Option<u32>,
    /// Hours a turn waits for orders before it runs anyway. Default 24.
    #[serde(default)]
    pub deadline_hours: Option<u32>,
    /// The map's seed; random if left out.
    #[serde(default)]
    pub seed: Option<u64>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct NewCrew {
    pub name: String,
    /// Played by the server's bot instead of a person.
    #[serde(default)]
    pub bot: bool,
}

// ------------------------------------------------------------ responses

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GameInfo {
    pub id: String,
    pub name: String,
    pub turn_days: u32,
    pub deadline_hours: u32,
}

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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewStatus {
    pub game: GameInfo,
    pub player: PlayerId,
    pub name: String,
    pub turn: u32,
    pub day: u32,
    /// Unix time the running turn resolves at the latest.
    pub deadline: i64,
    pub submitted: bool,
    /// The crew's own orders for the running turn.
    pub orders: Vec<Command>,
    pub crews: Vec<CrewState>,
    pub view: CrewView,
    /// The last resolved turn, as this crew may see it.
    pub last_turn: Option<CrewTurn>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewTurn {
    pub turn: u32,
    pub orders: Vec<Command>,
    pub report: TurnReport,
    pub resolved_at: i64,
}

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

// ------------------------------------------------------------ server

struct Crew {
    row: CrewRow,
    game: GameRow,
}

impl Server {
    pub fn open(path: impl AsRef<std::path::Path>) -> Result<Server, Error> {
        Ok(Server {
            data: GameData::classic(),
            db: Mutex::new(Store::open(path)?),
        })
    }

    pub fn data(&self) -> &GameData {
        &self.data
    }

    fn store(&self) -> std::sync::MutexGuard<'_, Store> {
        self.db
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn crew(store: &Store, token: &str) -> Result<Crew, Error> {
        let row = store
            .crew_by_token(token)?
            .ok_or_else(|| Error::NotFound("no crew has that token".into()))?;
        let game = store
            .game(&row.game_id)?
            .ok_or_else(|| Error::Internal("the crew's game is missing".into()))?;
        Ok(Crew { row, game })
    }

    pub fn create_game(&self, request: CreateGame, now: i64) -> Result<GameCreated, Error> {
        let name = request.name.trim();
        if name.is_empty() {
            return Err(Error::BadRequest("the game needs a name".into()));
        }
        if !(1..=4).contains(&request.crews.len()) {
            return Err(Error::BadRequest("a game takes 1 to 4 crews".into()));
        }
        if request.crews.iter().all(|c| c.bot) {
            return Err(Error::BadRequest(
                "at least one crew must be a person".into(),
            ));
        }
        if request.crews.iter().any(|c| c.name.trim().is_empty()) {
            return Err(Error::BadRequest("every crew needs a name".into()));
        }
        let turn_days = request.turn_days.unwrap_or(10);
        if !(1..=100).contains(&turn_days) {
            return Err(Error::BadRequest("a turn runs 1 to 100 days".into()));
        }
        let deadline_hours = request.deadline_hours.unwrap_or(24);
        let seed = request.seed.unwrap_or_else(rand::random);

        let id = format!("g_{}", token(8));
        let crews: Vec<CrewRow> = request
            .crews
            .iter()
            .enumerate()
            .map(|(index, crew)| CrewRow {
                game_id: id.clone(),
                player: PlayerId(index as u8),
                name: crew.name.trim().to_string(),
                bot: crew.bot,
                token: (!crew.bot).then(|| token(16)),
            })
            .collect();
        let named: Vec<(PlayerId, &str)> =
            crews.iter().map(|c| (c.player, c.name.as_str())).collect();
        let game = GameRow {
            id: id.clone(),
            name: name.to_string(),
            seed,
            turn_days,
            deadline_hours,
            created_at: now,
            turn: 0,
            world: World::new_game(&self.data, seed, &named),
            deadline: now + i64::from(deadline_hours) * 3600,
        };
        self.store().insert_game(&game, &crews)?;
        Ok(GameCreated {
            game: info(&game),
            crews: crews.into_iter().map(invite).collect(),
        })
    }

    pub fn crew_status(&self, token: &str) -> Result<CrewStatus, Error> {
        let store = self.store();
        let Crew { row, game } = Self::crew(&store, token)?;
        let orders = store.orders(&game.id, game.turn)?;
        let crews = store
            .crews(&game.id)?
            .into_iter()
            .map(|c| CrewState {
                submitted: c.bot || orders.contains_key(&c.player),
                player: c.player,
                name: c.name,
                bot: c.bot,
            })
            .collect();
        let last_turn = match game.turn.checked_sub(1) {
            Some(previous) => store
                .turn(&game.id, previous)?
                .map(|t| crew_turn(&t, row.player)),
            None => None,
        };
        let view = crew_view(&game.world, row.player, self.data.hideout.host)
            .ok_or_else(|| Error::Internal("the crew is not in its game".into()))?;
        Ok(CrewStatus {
            game: info(&game),
            player: row.player,
            name: row.name,
            turn: game.turn,
            day: game.world.day,
            deadline: game.deadline,
            submitted: orders.contains_key(&row.player),
            orders: orders.get(&row.player).cloned().unwrap_or_default(),
            crews,
            view,
            last_turn,
        })
    }

    /// Takes a crew's orders for the running turn, replacing any handed in
    /// before, and runs the turn if everyone has now handed in.
    pub fn submit_orders(
        &self,
        token: &str,
        orders: Vec<Command>,
        now: i64,
    ) -> Result<OrdersReceipt, Error> {
        let mut store = self.store();
        let Crew { row, game } = Self::crew(&store, token)?;

        // Try the orders on a copy so the crew hears at once what the rules
        // would refuse. The real turn may still differ: rivals go too.
        let mut preview = game.world.clone();
        let report = resolve_turn(
            &self.data,
            &mut preview,
            &Orders::from([(row.player, orders.clone())]),
            0,
        );
        let rejected = report
            .rejected
            .iter()
            .map(|r| RejectedOrder {
                index: r.index,
                error: r.error.to_string(),
            })
            .collect();

        store.put_orders(&game.id, game.turn, row.player, &orders, now)?;
        let resolved = self.resolve_if_ready(&mut store, &game.id, now, false)?;
        Ok(OrdersReceipt { rejected, resolved })
    }

    pub fn withdraw_orders(&self, token: &str) -> Result<(), Error> {
        let store = self.store();
        let Crew { row, game } = Self::crew(&store, token)?;
        store.delete_orders(&game.id, game.turn, row.player)?;
        Ok(())
    }

    pub fn turns(&self, token: &str) -> Result<Vec<TurnSummary>, Error> {
        let store = self.store();
        let Crew { game, .. } = Self::crew(&store, token)?;
        Ok(store.turns(&game.id)?)
    }

    pub fn turn(&self, token: &str, turn: u32) -> Result<CrewTurn, Error> {
        let store = self.store();
        let Crew { row, game } = Self::crew(&store, token)?;
        let row_turn = store
            .turn(&game.id, turn)?
            .ok_or_else(|| Error::NotFound(format!("turn {turn} has not run")))?;
        Ok(crew_turn(&row_turn, row.player))
    }

    /// Runs every turn whose deadline has passed; returns how many ran.
    pub fn resolve_due(&self, now: i64) -> Result<usize, Error> {
        let mut store = self.store();
        let mut ran = 0;
        for id in store.due_games(now)? {
            if self.resolve_if_ready(&mut store, &id, now, true)? {
                ran += 1;
            }
        }
        Ok(ran)
    }

    /// Runs the game's turn if every person has handed in, or if `force`
    /// (the deadline passed). Crews without orders give none; bots play.
    fn resolve_if_ready(
        &self,
        store: &mut Store,
        game_id: &str,
        now: i64,
        force: bool,
    ) -> Result<bool, Error> {
        let game = store
            .game(game_id)?
            .ok_or_else(|| Error::NotFound("no such game".into()))?;
        let crews = store.crews(game_id)?;
        let handed_in = store.orders(game_id, game.turn)?;
        let everyone = crews
            .iter()
            .all(|c| c.bot || handed_in.contains_key(&c.player));
        if !(force || everyone) {
            return Ok(false);
        }

        let mut orders: Orders = BTreeMap::new();
        for crew in &crews {
            let given = if crew.bot {
                bot::orders(&self.data, &game.world, crew.player)
            } else {
                handed_in.get(&crew.player).cloned().unwrap_or_default()
            };
            orders.insert(crew.player, given);
        }
        let mut world = game.world.clone();
        let report = resolve_turn(&self.data, &mut world, &orders, game.turn_days);
        let record = TurnRow {
            turn: game.turn,
            world_before: game.world,
            orders,
            report,
            resolved_at: now,
        };
        let deadline = now + i64::from(game.deadline_hours) * 3600;
        store.finish_turn(game_id, &record, &world, deadline)?;
        Ok(true)
    }
}

fn info(game: &GameRow) -> GameInfo {
    GameInfo {
        id: game.id.clone(),
        name: game.name.clone(),
        turn_days: game.turn_days,
        deadline_hours: game.deadline_hours,
    }
}

fn invite(crew: CrewRow) -> CrewInvite {
    CrewInvite {
        player: crew.player,
        name: crew.name,
        bot: crew.bot,
        join_path: crew.token.as_ref().map(|t| format!("/join/{t}")),
        token: crew.token,
    }
}

fn crew_turn(turn: &TurnRow, player: PlayerId) -> CrewTurn {
    CrewTurn {
        turn: turn.turn,
        orders: turn.orders.get(&player).cloned().unwrap_or_default(),
        report: turn.report.for_crew(player),
        resolved_at: turn.resolved_at,
    }
}

/// A random lower-case hex string of `bytes` bytes.
fn token(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::fill(&mut buf[..]);
    buf.iter().map(|b| format!("{b:02x}")).collect()
}
