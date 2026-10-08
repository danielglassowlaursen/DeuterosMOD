//! The HTTP API, and the pages and client the server hands out.
//!
//! Every crew is identified by the secret token in its invite link, so
//! there are no accounts: `/api/crew/{token}` is that crew's own window on
//! its game, and nothing a rival hands in is ever sent through it.

use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use nullnet_core::Command;
use serde_json::json;
use tower_http::services::ServeDir;

use crate::games::{self, CreateGame, Error, Server};

const CONSOLE: &str = include_str!("../../../web/console.html");

pub fn router(server: Arc<Server>, web_dir: Option<PathBuf>) -> Router {
    let api = Router::new()
        .route("/games", post(create_game))
        .route("/crew/{token}", get(crew_status))
        .route(
            "/crew/{token}/orders",
            axum::routing::put(submit_orders).delete(withdraw_orders),
        )
        .route("/crew/{token}/turns", get(turns))
        .route("/crew/{token}/turns/{turn}", get(turn))
        .with_state(server);

    let pages = Router::new()
        .route("/console", get(console))
        .route("/join/{token}", get(console));

    let app = Router::new().nest("/api", api).merge(pages);
    match web_dir {
        Some(dir) => app.fallback_service(ServeDir::new(dir)),
        None => app.route("/", get(|| async { Redirect::temporary("/console") })),
    }
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

async fn create_game(
    State(server): State<Arc<Server>>,
    Json(request): Json<CreateGame>,
) -> Result<(StatusCode, Json<games::GameCreated>), Error> {
    let created = server.create_game(request, games::now())?;
    Ok((StatusCode::CREATED, Json(created)))
}

async fn crew_status(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
) -> Result<Json<games::CrewStatus>, Error> {
    Ok(Json(server.crew_status(&token)?))
}

async fn submit_orders(
    State(server): State<Arc<Server>>,
    Path(token): Path<String>,
    Json(orders): Json<Vec<Command>>,
) -> Result<Json<games::OrdersReceipt>, Error> {
    Ok(Json(server.submit_orders(&token, orders, games::now())?))
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
) -> Result<Json<Vec<crate::db::TurnSummary>>, Error> {
    Ok(Json(server.turns(&token)?))
}

async fn turn(
    State(server): State<Arc<Server>>,
    Path((token, turn)): Path<(String, u32)>,
) -> Result<Json<games::CrewTurn>, Error> {
    Ok(Json(server.turn(&token, turn)?))
}
