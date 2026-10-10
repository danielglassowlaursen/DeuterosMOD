//! The SQLite store. Every game keeps its current world, the orders handed
//! in for the running turn, and a record of every resolved turn: the world
//! before it, the orders and the report, so any turn can be replayed.
//!
//! The world's serialised form changed when the rules were rebuilt, so the
//! store carries a schema version. When it opens an older store it drops the
//! game data (which it could not read anyway) and starts fresh.

use std::collections::BTreeMap;
use std::path::Path;

use nullnet_api::TurnSummary;
use nullnet_core::{Command, Orders, PlayerId, TurnReport, World};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub type DbResult<T> = Result<T, rusqlite::Error>;

/// Bumped whenever the stored world or turn format changes.
const SCHEMA_VERSION: i64 = 2;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS games (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    seed INTEGER NOT NULL,
    difficulty TEXT NOT NULL,
    last_turn INTEGER NOT NULL,
    deadline_hours INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    world TEXT NOT NULL,
    deadline INTEGER NOT NULL,
    notify_url TEXT
);
CREATE TABLE IF NOT EXISTS crews (
    game_id TEXT NOT NULL REFERENCES games(id),
    player INTEGER NOT NULL,
    name TEXT NOT NULL,
    bot INTEGER NOT NULL,
    token TEXT UNIQUE,
    PRIMARY KEY (game_id, player)
);
CREATE TABLE IF NOT EXISTS orders (
    game_id TEXT NOT NULL REFERENCES games(id),
    turn INTEGER NOT NULL,
    player INTEGER NOT NULL,
    orders TEXT NOT NULL,
    submitted_at INTEGER NOT NULL,
    PRIMARY KEY (game_id, turn, player)
);
CREATE TABLE IF NOT EXISTS turns (
    game_id TEXT NOT NULL REFERENCES games(id),
    turn INTEGER NOT NULL,
    world_before TEXT NOT NULL,
    orders TEXT NOT NULL,
    report TEXT NOT NULL,
    resolved_at INTEGER NOT NULL,
    PRIMARY KEY (game_id, turn)
);
";

pub struct Store {
    conn: Connection,
}

#[derive(Clone, Debug)]
pub struct GameRow {
    pub id: String,
    pub name: String,
    pub seed: u64,
    pub difficulty: String,
    pub last_turn: u32,
    pub deadline_hours: u32,
    pub created_at: i64,
    pub world: World,
    /// Unix time the running turn resolves whether or not everyone has
    /// handed in orders.
    pub deadline: i64,
    /// A webhook to post to when a turn has run.
    pub notify_url: Option<String>,
}

impl GameRow {
    /// The turn now being played.
    pub fn turn(&self) -> u32 {
        self.world.turn
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CrewRow {
    pub game_id: String,
    pub player: PlayerId,
    pub name: String,
    pub bot: bool,
    /// The secret in the crew's invite link; bots have none.
    pub token: Option<String>,
}

#[derive(Clone, Debug)]
pub struct TurnRow {
    pub turn: u32,
    pub world_before: World,
    pub orders: Orders,
    pub report: TurnReport,
    pub resolved_at: i64,
}

/// Clears game data left by an older, incompatible build.
fn reset_if_old(conn: &Connection) -> DbResult<()> {
    let version: i64 = conn.query_row("PRAGMA user_version", [], |row| row.get(0))?;
    if version != SCHEMA_VERSION {
        conn.execute_batch(
            "DROP TABLE IF EXISTS turns;
             DROP TABLE IF EXISTS orders;
             DROP TABLE IF EXISTS crews;
             DROP TABLE IF EXISTS games;",
        )?;
        conn.execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION}"))?;
    }
    Ok(())
}

fn json<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).expect("game state serialises")
}

fn parse<T: for<'de> Deserialize<'de>>(text: String) -> DbResult<T> {
    serde_json::from_str(&text).map_err(|e| {
        rusqlite::Error::FromSqlConversionFailure(0, rusqlite::types::Type::Text, Box::new(e))
    })
}

impl Store {
    /// Opens or creates the store; `:memory:` keeps it in memory.
    pub fn open(path: impl AsRef<Path>) -> DbResult<Store> {
        let conn = Connection::open(path)?;
        conn.execute_batch("PRAGMA journal_mode = WAL; PRAGMA foreign_keys = ON;")?;
        reset_if_old(&conn)?;
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn insert_game(&mut self, game: &GameRow, crews: &[CrewRow]) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO games (id, name, seed, difficulty, last_turn, deadline_hours, created_at, world, deadline, notify_url)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            params![
                game.id,
                game.name,
                game.seed as i64,
                game.difficulty,
                game.last_turn,
                game.deadline_hours,
                game.created_at,
                json(&game.world),
                game.deadline,
                game.notify_url
            ],
        )?;
        for crew in crews {
            tx.execute(
                "INSERT INTO crews (game_id, player, name, bot, token) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![crew.game_id, crew.player.0, crew.name, crew.bot, crew.token],
            )?;
        }
        tx.commit()
    }

    pub fn game(&self, id: &str) -> DbResult<Option<GameRow>> {
        self.conn
            .query_row(
                "SELECT id, name, seed, difficulty, last_turn, deadline_hours, created_at, world, deadline, notify_url
                 FROM games WHERE id = ?1",
                params![id],
                |row| {
                    Ok(GameRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        seed: row.get::<_, i64>(2)? as u64,
                        difficulty: row.get(3)?,
                        last_turn: row.get(4)?,
                        deadline_hours: row.get(5)?,
                        created_at: row.get(6)?,
                        world: parse(row.get(7)?)?,
                        deadline: row.get(8)?,
                        notify_url: row.get(9)?,
                    })
                },
            )
            .optional()
    }

    pub fn crew_by_token(&self, token: &str) -> DbResult<Option<CrewRow>> {
        self.conn
            .query_row(
                "SELECT game_id, player, name, bot, token FROM crews WHERE token = ?1",
                params![token],
                crew_row,
            )
            .optional()
    }

    pub fn crews(&self, game_id: &str) -> DbResult<Vec<CrewRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT game_id, player, name, bot, token FROM crews WHERE game_id = ?1 ORDER BY player",
        )?;
        let rows = stmt.query_map(params![game_id], crew_row)?;
        rows.collect()
    }

    pub fn put_orders(
        &self,
        game_id: &str,
        turn: u32,
        player: PlayerId,
        orders: &[Command],
        now: i64,
    ) -> DbResult<()> {
        self.conn.execute(
            "INSERT INTO orders (game_id, turn, player, orders, submitted_at) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (game_id, turn, player) DO UPDATE SET orders = ?4, submitted_at = ?5",
            params![game_id, turn, player.0, json(&orders), now],
        )?;
        Ok(())
    }

    pub fn delete_orders(&self, game_id: &str, turn: u32, player: PlayerId) -> DbResult<()> {
        self.conn.execute(
            "DELETE FROM orders WHERE game_id = ?1 AND turn = ?2 AND player = ?3",
            params![game_id, turn, player.0],
        )?;
        Ok(())
    }

    /// The orders handed in for a turn so far, by player.
    pub fn orders(&self, game_id: &str, turn: u32) -> DbResult<BTreeMap<PlayerId, Vec<Command>>> {
        let mut stmt = self
            .conn
            .prepare("SELECT player, orders FROM orders WHERE game_id = ?1 AND turn = ?2")?;
        let rows = stmt.query_map(params![game_id, turn], |row| {
            Ok((PlayerId(row.get(0)?), parse(row.get(1)?)?))
        })?;
        rows.collect()
    }

    /// Records a resolved turn and moves the game on to the next one.
    pub fn finish_turn(
        &mut self,
        game_id: &str,
        turn: &TurnRow,
        world_after: &World,
        deadline: i64,
    ) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO turns (game_id, turn, world_before, orders, report, resolved_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![
                game_id,
                turn.turn,
                json(&turn.world_before),
                json(&turn.orders),
                json(&turn.report),
                turn.resolved_at
            ],
        )?;
        tx.execute(
            "UPDATE games SET world = ?2, deadline = ?3 WHERE id = ?1",
            params![game_id, json(world_after), deadline],
        )?;
        tx.execute(
            "DELETE FROM orders WHERE game_id = ?1 AND turn = ?2",
            params![game_id, turn.turn],
        )?;
        tx.commit()
    }

    pub fn turns(&self, game_id: &str) -> DbResult<Vec<TurnSummary>> {
        let mut stmt = self
            .conn
            .prepare("SELECT turn, resolved_at FROM turns WHERE game_id = ?1 ORDER BY turn")?;
        let rows = stmt.query_map(params![game_id], |row| {
            Ok(TurnSummary {
                turn: row.get(0)?,
                resolved_at: row.get(1)?,
            })
        })?;
        rows.collect()
    }

    pub fn turn(&self, game_id: &str, turn: u32) -> DbResult<Option<TurnRow>> {
        self.conn
            .query_row(
                "SELECT turn, world_before, orders, report, resolved_at FROM turns
                 WHERE game_id = ?1 AND turn = ?2",
                params![game_id, turn],
                |row| {
                    Ok(TurnRow {
                        turn: row.get(0)?,
                        world_before: parse(row.get(1)?)?,
                        orders: parse(row.get(2)?)?,
                        report: parse(row.get(3)?)?,
                        resolved_at: row.get(4)?,
                    })
                },
            )
            .optional()
    }

    /// Games whose running turn's deadline has passed and that are not over.
    pub fn due_games(&self, now: i64) -> DbResult<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM games WHERE deadline <= ?1 ORDER BY deadline")?;
        let rows = stmt.query_map(params![now], |row| row.get(0))?;
        rows.collect()
    }
}

fn crew_row(row: &rusqlite::Row<'_>) -> DbResult<CrewRow> {
    Ok(CrewRow {
        game_id: row.get(0)?,
        player: PlayerId(row.get(1)?),
        name: row.get(2)?,
        bot: row.get(3)?,
        token: row.get(4)?,
    })
}
