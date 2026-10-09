//! Webhook notices: a line posted to a Discord or Slack style webhook when
//! a turn has run, so the crews hear it where they already are. The body
//! carries both `content` (Discord) and `text` (Slack).

use nullnet_core::{EndReason, World, date};
use serde::Serialize;

use crate::db::CrewRow;

/// One post waiting to go out.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub url: String,
    pub text: String,
}

#[derive(Serialize)]
struct Body<'a> {
    content: &'a str,
    text: &'a str,
}

/// The line for a turn that has just run, or for the game's end.
pub fn turn_text(game: &str, turn: u32, world: &World, crews: &[CrewRow]) -> String {
    let name = |player| {
        crews
            .iter()
            .find(|c| c.player == player)
            .map_or("a crew".to_string(), |c| c.name.clone())
    };
    match &world.ended {
        Some(end) => {
            let how = match end.reason {
                EndReason::Domination => "by holding most of the home network",
                EndReason::DayLimit => "on points at the last day",
            };
            let standings: Vec<String> = end
                .scores
                .iter()
                .map(|s| format!("{} {}", name(s.player), s.total))
                .collect();
            format!(
                "NullNet: {game} is over on {}. {} wins {how}. Final: {}.",
                date(end.day),
                name(end.winner),
                standings.join(", ")
            )
        }
        None => {
            let people: Vec<String> = crews
                .iter()
                .filter(|c| !c.bot)
                .map(|c| c.name.clone())
                .collect();
            format!(
                "NullNet: turn {turn} of {game} has run; it is {}. Orders for turn {} are open{}.",
                date(world.day),
                turn + 1,
                if people.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", people.join(", "))
                }
            )
        }
    }
}

/// Posts every notice, each on its own blocking thread, and logs failures.
pub async fn deliver(notices: Vec<Notice>) {
    for notice in notices {
        let result = tokio::task::spawn_blocking(move || post(&notice)).await;
        match result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => eprintln!("webhook failed: {error}"),
            Err(error) => eprintln!("webhook task failed: {error}"),
        }
    }
}

fn post(notice: &Notice) -> Result<(), String> {
    let body = serde_json::to_string(&Body {
        content: &notice.text,
        text: &notice.text,
    })
    .map_err(|e| e.to_string())?;
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(std::time::Duration::from_secs(10)))
        .build()
        .into();
    agent
        .post(&notice.url)
        .header("content-type", "application/json")
        .send(body.as_bytes())
        .map(|_| ())
        .map_err(|e| format!("{}: {e}", notice.url))
}
