//! The SQLite store. Every game keeps its current world, the orders handed
//! in for the running turn, and a record of every resolved turn: the world
//! before it, the orders and the report, so any turn can be replayed.

use std::collections::BTreeMap;
use std::path::Path;

use nullnet_core::{Command, Orders, PlayerId, TurnReport, World};
use rusqlite::{Connection, OptionalExtension, params};
use serde::{Deserialize, Serialize};

pub type DbResult<T> = Result<T, rusqlite::Error>;

const SCHEMA: &str = "
CREATE TABLE IF NOT EXISTS games (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    seed INTEGER NOT NULL,
    turn_days INTEGER NOT NULL,
    deadline_hours INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    turn INTEGER NOT NULL,
    world TEXT NOT NULL,
    deadline INTEGER NOT NULL
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
    pub turn_days: u32,
    pub deadline_hours: u32,
    pub created_at: i64,
    pub turn: u32,
    pub world: World,
    /// Unix time the running turn resolves whether or not everyone has
    /// handed in orders.
    pub deadline: i64,
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

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TurnSummary {
    pub turn: u32,
    pub first_day: u32,
    pub last_day: u32,
    pub resolved_at: i64,
}

#[derive(Clone, Debug)]
pub struct TurnRow {
    pub turn: u32,
    pub world_before: World,
    pub orders: Orders,
    pub report: TurnReport,
    pub resolved_at: i64,
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
        conn.execute_batch(SCHEMA)?;
        Ok(Store { conn })
    }

    pub fn insert_game(&mut self, game: &GameRow, crews: &[CrewRow]) -> DbResult<()> {
        let tx = self.conn.transaction()?;
        tx.execute(
            "INSERT INTO games (id, name, seed, turn_days, deadline_hours, created_at, turn, world, deadline)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                game.id,
                game.name,
                game.seed as i64,
                game.turn_days,
                game.deadline_hours,
                game.created_at,
                game.turn,
                json(&game.world),
                game.deadline
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
                "SELECT id, name, seed, turn_days, deadline_hours, created_at, turn, world, deadline
                 FROM games WHERE id = ?1",
                params![id],
                |row| {
                    Ok(GameRow {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        seed: row.get::<_, i64>(2)? as u64,
                        turn_days: row.get(3)?,
                        deadline_hours: row.get(4)?,
                        created_at: row.get(5)?,
                        turn: row.get(6)?,
                        world: parse(row.get(7)?)?,
                        deadline: row.get(8)?,
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
            "UPDATE games SET turn = ?2, world = ?3, deadline = ?4 WHERE id = ?1",
            params![game_id, world_after.turn, json(world_after), deadline],
        )?;
        tx.execute(
            "DELETE FROM orders WHERE game_id = ?1 AND turn = ?2",
            params![game_id, turn.turn],
        )?;
        tx.commit()
    }

    pub fn turns(&self, game_id: &str) -> DbResult<Vec<TurnSummary>> {
        let mut stmt = self.conn.prepare(
            "SELECT turn, report, resolved_at FROM turns WHERE game_id = ?1 ORDER BY turn",
        )?;
        let rows = stmt.query_map(params![game_id], |row| {
            let report: TurnReport = parse(row.get(1)?)?;
            Ok(TurnSummary {
                turn: row.get(0)?,
                first_day: report.first_day,
                last_day: report.last_day,
                resolved_at: row.get(2)?,
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

    /// Games whose running turn's deadline has passed.
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
