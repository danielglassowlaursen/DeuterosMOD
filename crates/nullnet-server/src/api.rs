//! The HTTP API, and the pages and client the server hands out.
//!
//! Every crew is identified by the secret token in its invite link, so
//! there are no accounts: `/api/crew/{token}` is that crew's own window on
//! its game, and nothing a rival hands in is ever sent through it.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::http::header::{CACHE_CONTROL, HeaderValue};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nullnet_core::{Command, GameData};
use serde_json::json;
use tower_http::services::ServeDir;
use tower_http::set_header::SetResponseHeaderLayer;

use nullnet_api::{CreateGame, CrewStatus, CrewTurn, GameCreated, OrdersReceipt, TurnSummary};

use crate::games::{self, Error, Server};
use crate::notify;

const CONSOLE: &str = include_str!("../../../web/console.html");

pub fn router(server: Arc<Server>, web_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/games", post(create_game))
        .route("/map", get(map))
        .route("/crew/{token}", get(crew_status))
        .route("/crew/{token}/map", get(crew_map))
        .route(
            "/crew/{token}/orders",
            axum::routing::put(submit_orders).delete(withdraw_orders),
        )
        .route("/crew/{token}/turns", get(turns))
        .route("/crew/{token}/turns/{turn}", get(turn))
        .with_state(server);

    // An invite link opens the client when the server has one to serve,
    // otherwise the console; the console is always at /console.
    let pages = Router::new()
        .route("/console", get(console))
        .route("/join/{token}", get(join))
        .with_state(web_dir.clone());

    let app = Router::new().nest("/api", api).merge(pages);
    let app = match web_dir {
        Some(dir) => app.fallback_service(ServeDir::new(dir)),
        None => app.route("/", get(|| async { Redirect::temporary("/console") })),
    };
    // The client's files keep the same names across builds, so tell the
    // browser to revalidate rather than serve a stale client against a new
    // server (which then fails to parse the answer).
    app.layer(SetResponseHeaderLayer::overriding(
        CACHE_CONTROL,
        HeaderValue::from_static("no-cache"),
    ))
}

async fn join(State(web_dir): State<Option<PathBuf>>) -> Html<String> {
    if let Some(dir) = web_dir
        && let Ok(page) = tokio::fs::read_to_string(dir.join("index.html")).await
    {
        return Html(page);
    }
    Html(CONSOLE.to_string())
}

impl IntoResponse for Error {
    fn into_response(self) -> Response {
        let status = match &self {
            Error::NotFound(_) => StatusCode::NOT_FOUND,
            Error::BadRequest(_) => StatusCode::BAD_REQUEST,
            Error::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        (status, Json(json!({ "error": self.to_string() }))).into_response()
    }
}

async fn console() -> Html<&'static str> {
    Html(CONSOLE)
}

/// The standard map: host names, places and links.
async fn map(State(server): State<Arc<Server>>) -> Json<serde_json::Value> {
    Json(map_json(server.data()))
}

/// The map of a crew's game, standard or random.
async fn crew_map(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Result<Json<serde_json::Value>, Error> {
    let data = server.crew_map(&token)?;
    Ok(Json(map_json(&data)))
}

fn map_json(data: &GameData) -> serde_json::Value {
    json!({
        "hosts": data.hosts,
        "links": data.links.iter().map(|&(a, b)| [a.0, b.0]).collect::<Vec<_>>(),
    })
}

async fn create_game(
    State(server): State<Arc<Server>>,
    Json(request): Json<CreateGame>,
) -> Result<(StatusCode, Json<GameCreated>), Error> {
    let created = server.create_game(request, games::now())?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn crew_status(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Result<Json<CrewStatus>, Error> {
    Ok(Json(server.crew_status(&token, games::now())?))
}

async fn submit_orders(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
    Json(orders): Json<Vec<Command>>,
) -> Result<Json<OrdersReceipt>, Error> {
    let receipt = server.submit_orders(&token, orders, games::now())?;
    // A hand-in that ran the turn may have queued a webhook post.
    let notices = server.take_notices();
    if !notices.is_empty() {
        tokio::spawn(notify::deliver(notices));
    }
    Ok(Json(receipt))
}

async fn withdraw_orders(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Result<StatusCode, Error> {
    server.withdraw_orders(&token)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn turns(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Result<Json<Vec<TurnSummary>>, Error> {
    Ok(Json(server.turns(&token)?))
}

async fn turn(
    State(server): State<Arc<Server>>,
    Path((token, turn)): Path<(String, u32)>,
) -> Result<Json<CrewTurn>, Error> {
    Ok(Json(server.turn(&token, turn)?))
}
