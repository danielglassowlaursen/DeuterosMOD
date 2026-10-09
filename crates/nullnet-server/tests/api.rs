//! The server's API, driven through the router in process: games are
//! created, crews hand in orders, turns run when everyone has or when the
//! deadline passes, bots play their seats, and everything survives a
//! restart.

use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use http_body_util::BodyExt;
use nullnet_server::{Server, router};
use serde_json::{Value, json};
use tower::ServiceExt;

struct Client {
    server: Arc<Server>,
}

impl Client {
    fn in_memory() -> Client {
        Client {
            server: Arc::new(Server::open(":memory:").unwrap()),
        }
    }

    fn app(&self) -> Router {
        router(Arc::clone(&self.server), None)
    }

    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> (StatusCode, Value) {
        let request = Request::builder()
            .method(method)
            .uri(path)
            .header(header::CONTENT_TYPE, "application/json")
            .body(match body {
                Some(json) => Body::from(json.to_string()),
                None => Body::empty(),
            })
            .unwrap();
        let response = self.app().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let value = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, value)
    }

    async fn get(&self, path: &str) -> (StatusCode, Value) {
        self.call(Method::GET, path, None).await
    }

    async fn create(&self, crews: Value, deadline_hours: u32) -> Value {
        let (status, created) = self
            .call(
                Method::POST,
                "/api/games",
                Some(json!({
                    "name": "Test run",
                    "crews": crews,
                    "turn_days": 10,
                    "deadline_hours": deadline_hours,
                    "seed": 7
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED, "{created}");
        created
    }
}

fn token(created: &Value, index: usize) -> String {
    created["crews"][index]["token"]
        .as_str()
        .unwrap()
        .to_string()
}

fn recruit(kind: &str, count: u32) -> Value {
    json!({ "Recruit": { "kind": kind, "count": count } })
}

#[tokio::test]
async fn a_game_hands_out_one_invite_per_person_and_none_to_bots() {
    let client = Client::in_memory();
    let created = client
        .create(
            json!([{ "name": "Ghostline" }, { "name": "Blackice", "bot": true }, { "name": "Nullsector" }]),
            24,
        )
        .await;
    assert!(created["id"].as_str().unwrap().starts_with("g_"));
    assert_eq!(created["crews"].as_array().unwrap().len(), 3);
    assert!(created["crews"][0]["token"].is_string());
    assert!(created["crews"][1]["token"].is_null());
    assert_eq!(
        created["crews"][0]["join_path"].as_str().unwrap(),
        format!("/join/{}", token(&created, 0))
    );

    let (status, me) = client
        .get(&format!("/api/crew/{}", token(&created, 0)))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["name"], "Ghostline");
    assert_eq!(me["turn"], 0);
    assert_eq!(me["day"], 0);
    assert_eq!(me["submitted"], false);
    assert_eq!(me["crews"][1]["submitted"], true, "a bot is always ready");
    assert_eq!(me["view"]["me"]["hideout"]["taps"], 1);
    assert!(me["last_turn"].is_null());

    let (status, _) = client.get("/api/crew/not-a-token").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn games_need_a_name_a_person_and_one_to_four_crews() {
    let client = Client::in_memory();
    for body in [
        json!({ "name": " ", "crews": [{ "name": "A" }] }),
        json!({ "name": "x", "crews": [] }),
        json!({ "name": "x", "crews": [{ "name": "A", "bot": true }] }),
        json!({ "name": "x", "crews": [{ "name": "A" }, { "name": "B" }, { "name": "C" }, { "name": "D" }, { "name": "E" }] }),
        json!({ "name": "x", "crews": [{ "name": "A" }], "turn_days": 0 }),
    ] {
        let (status, _) = client.call(Method::POST, "/api/games", Some(body)).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}

#[tokio::test]
async fn the_turn_runs_once_every_person_has_handed_in() {
    let client = Client::in_memory();
    let created = client
        .create(
            json!([{ "name": "Ghostline" }, { "name": "Blackice" }, { "name": "Bot", "bot": true }]),
            24,
        )
        .await;
    let (a, b) = (token(&created, 0), token(&created, 1));

    let (status, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{a}/orders"),
            Some(json!([recruit("Analyst", 100), recruit("Coder", 100)])),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["rejected"].as_array().unwrap().len(), 0);
    assert_eq!(receipt["resolved"], false);

    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["submitted"], true);
    assert_eq!(me["orders"].as_array().unwrap().len(), 2);
    assert_eq!(me["crews"][1]["submitted"], false);
    let (_, rival) = client.get(&format!("/api/crew/{b}")).await;
    assert_eq!(rival["crews"][0]["submitted"], true);
    assert!(
        rival["orders"].as_array().unwrap().is_empty(),
        "a crew never sees a rival's orders"
    );

    let (_, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{b}/orders"),
            Some(json!([recruit("Operator", 20)])),
        )
        .await;
    assert_eq!(receipt["resolved"], true);

    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["turn"], 1);
    assert_eq!(me["day"], 10);
    assert_eq!(me["submitted"], false);
    assert_eq!(me["last_turn"]["turn"], 0);
    assert_eq!(me["last_turn"]["orders"].as_array().unwrap().len(), 2);
    assert_eq!(me["last_turn"]["report"]["first_day"], 1);
    assert_eq!(me["last_turn"]["report"]["last_day"], 10);
    assert_eq!(
        me["view"]["me"]["recruitment"]["courses"]["Analyst"]["enrolled"],
        100
    );

    let (_, turns) = client.get(&format!("/api/crew/{a}/turns")).await;
    assert_eq!(turns.as_array().unwrap().len(), 1);
    let (status, turn) = client.get(&format!("/api/crew/{b}/turns/0")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        turn["orders"].as_array().unwrap().len(),
        1,
        "Blackice's own orders"
    );
    let (status, _) = client.get(&format!("/api/crew/{b}/turns/5")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn the_game_ends_on_its_last_day_and_takes_no_more_orders() {
    let client = Client::in_memory();
    let (status, created) = client
        .call(
            Method::POST,
            "/api/games",
            Some(json!({
                "name": "Short run",
                "crews": [{ "name": "Ghostline" }, { "name": "Bot", "bot": true }],
                "turn_days": 10,
                "deadline_hours": 24,
                "end_day": 20,
                "seed": 7
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["end_day"], 20);
    let me = token(&created, 0);

    let (_, status) = client.get(&format!("/api/crew/{me}")).await;
    assert_eq!(status["game"]["end_day"], 20);
    assert_eq!(status["over"], false);
    assert_eq!(status["view"]["scores"].as_array().unwrap().len(), 2);
    assert!(status["view"]["ended"].is_null());

    for _ in 0..2 {
        let (status, receipt) = client
            .call(
                Method::PUT,
                &format!("/api/crew/{me}/orders"),
                Some(json!([recruit("Analyst", 100)])),
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{receipt}");
        assert_eq!(receipt["resolved"], true);
    }
    let (_, status) = client.get(&format!("/api/crew/{me}")).await;
    assert_eq!(status["day"], 20);
    assert_eq!(status["over"], true);
    assert_eq!(status["view"]["ended"]["day"], 20);
    assert_eq!(status["view"]["ended"]["reason"], "DayLimit");
    let events = status["last_turn"]["report"]["events"].as_array().unwrap();
    assert!(
        events.iter().any(|e| e.get("GameOver").is_some()),
        "{events:?}"
    );

    let (status, error) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{me}/orders"),
            Some(json!([])),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{error}");

    // A last day within the first turn is refused.
    let (status, _) = client
        .call(
            Method::POST,
            "/api/games",
            Some(json!({
                "name": "Too short",
                "crews": [{ "name": "Ghostline" }],
                "turn_days": 10,
                "end_day": 5
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_turn_that_ran_is_posted_to_the_game_webhook() {
    use axum::Json;
    use axum::routing::post;
    use std::sync::Mutex;

    // A webhook of our own, on a free local port.
    let received: Arc<Mutex<Vec<Value>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&received);
    let hook = Router::new().route(
        "/hook",
        post(move |Json(body): Json<Value>| {
            let sink = Arc::clone(&sink);
            async move {
                sink.lock().unwrap().push(body);
                StatusCode::NO_CONTENT
            }
        }),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move { axum::serve(listener, hook).await.unwrap() });

    let client = Client::in_memory();
    let (status, created) = client
        .call(
            Method::POST,
            "/api/games",
            Some(json!({
                "name": "Hooked run",
                "crews": [{ "name": "Ghostline" }, { "name": "Bot", "bot": true }],
                "turn_days": 10,
                "end_day": 10,
                "notify_url": format!("http://{address}/hook"),
                "seed": 7
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(created["notifies"], true);
    let me = token(&created, 0);
    assert!(client.server.take_notices().is_empty());

    let (_, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{me}/orders"),
            Some(json!([])),
        )
        .await;
    assert_eq!(receipt["resolved"], true);
    // The handler spawned the delivery; take and deliver the queue here to
    // wait for it, or find it already sent.
    let notices = client.server.take_notices();
    nullnet_server::notify::deliver(notices).await;
    for _ in 0..50 {
        if !received.lock().unwrap().is_empty() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    let posts = received.lock().unwrap().clone();
    assert_eq!(posts.len(), 1, "{posts:?}");
    let text = posts[0]["content"].as_str().unwrap();
    assert_eq!(posts[0]["text"], posts[0]["content"]);
    assert!(text.contains("Hooked run"), "{text}");
    assert!(
        text.contains("is over") && text.contains("Ghostline wins"),
        "{text}"
    );

    // A webhook that is not a URL is refused.
    let (status, _) = client
        .call(
            Method::POST,
            "/api/games",
            Some(json!({
                "name": "Bad hook",
                "crews": [{ "name": "Ghostline" }],
                "notify_url": "discord.com/api/webhooks/1"
            })),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn handing_in_previews_what_the_rules_refuse_and_can_be_withdrawn() {
    let client = Client::in_memory();
    let created = client
        .create(json!([{ "name": "Ghostline" }, { "name": "Blackice" }]), 24)
        .await;
    let a = token(&created, 0);
    let (_, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{a}/orders"),
            Some(json!([
                { "Build": { "at": "Hideout", "item": "Tap" } },
                recruit("Analyst", 50)
            ])),
        )
        .await;
    assert_eq!(receipt["rejected"][0]["index"], 0);
    assert_eq!(
        receipt["rejected"][0]["error"],
        "the workshop has no coders"
    );
    assert_eq!(receipt["rejected"].as_array().unwrap().len(), 1);

    let (status, _) = client
        .call(Method::DELETE, &format!("/api/crew/{a}/orders"), None)
        .await;
    assert_eq!(status, StatusCode::NO_CONTENT);
    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["submitted"], false);
    assert!(me["orders"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn the_deadline_runs_the_turn_without_the_missing_orders() {
    let client = Client::in_memory();
    let created = client
        .create(
            json!([{ "name": "Ghostline" }, { "name": "Bot", "bot": true }]),
            1,
        )
        .await;
    let a = token(&created, 0);
    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    let deadline = me["deadline"].as_i64().unwrap();

    assert_eq!(client.server.resolve_due(deadline - 1).unwrap(), 0);
    assert_eq!(client.server.resolve_due(deadline).unwrap(), 1);
    assert_eq!(
        client.server.resolve_due(deadline).unwrap(),
        0,
        "the next deadline is an hour on"
    );

    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["turn"], 1);
    assert_eq!(me["day"], 10);
    assert_eq!(me["deadline"], deadline + 3600);
    assert!(me["last_turn"]["orders"].as_array().unwrap().is_empty());
}

#[tokio::test]
async fn bots_play_their_seats() {
    let client = Client::in_memory();
    let created = client
        .create(
            json!([{ "name": "Ghostline" }, { "name": "Bot", "bot": true }]),
            24,
        )
        .await;
    let a = token(&created, 0);
    for _ in 0..30 {
        let (_, receipt) = client
            .call(
                Method::PUT,
                &format!("/api/crew/{a}/orders"),
                Some(json!([])),
            )
            .await;
        assert_eq!(receipt["resolved"], true);
    }
    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["day"], 300);
    // The bot has recruited and researched; the person has done nothing.
    assert_eq!(me["view"]["me"]["research_team"], Value::Null);
    let researched = |crew: &Value| {
        crew["research"]
            .as_object()
            .unwrap()
            .values()
            .filter(|p| p["researched"] == true)
            .count()
    };
    assert_eq!(
        researched(&me["view"]["me"]),
        1,
        "only the starter research"
    );
    // The bot's hideout is hidden, but its milestones show through its
    // claims only, so check the record instead.
    let (_, turn) = client.get(&format!("/api/crew/{a}/turns/29")).await;
    assert!(
        turn["report"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .all(|e| {
                let (_, inner) = e.as_object().unwrap().iter().next().unwrap();
                inner["player"] == 0 || e.get("HostClaimed").is_some()
            }),
        "a crew only sees its own events and public ones"
    );
    let bot_world_turns = client.server.turns(&a).unwrap();
    assert_eq!(bot_world_turns.len(), 30);
}

#[tokio::test]
async fn games_survive_a_restart() {
    let dir = std::env::temp_dir().join(format!("nullnet-test-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("games.db");

    let created = {
        let client = Client {
            server: Arc::new(Server::open(&path).unwrap()),
        };
        let created = client
            .create(json!([{ "name": "Ghostline" }, { "name": "Blackice" }]), 24)
            .await;
        let a = token(&created, 0);
        client
            .call(
                Method::PUT,
                &format!("/api/crew/{a}/orders"),
                Some(json!([recruit("Coder", 10)])),
            )
            .await;
        created
    };

    let client = Client {
        server: Arc::new(Server::open(&path).unwrap()),
    };
    let (status, me) = client
        .get(&format!("/api/crew/{}", token(&created, 0)))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["game"]["name"], "Test run");
    assert_eq!(me["submitted"], true, "handed-in orders are kept");
    assert_eq!(me["orders"][0]["Recruit"]["count"], 10);
    let _ = std::fs::remove_dir_all(&dir);
}

#[tokio::test]
async fn the_console_and_invite_pages_are_served() {
    let client = Client::in_memory();
    for path in ["/console", "/join/abcdef0123"] {
        let request = Request::builder().uri(path).body(Body::empty()).unwrap();
        let response = client.app().oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let body = response.into_body().collect().await.unwrap().to_bytes();
        assert!(
            std::str::from_utf8(&body)
                .unwrap()
                .contains("NullNet console")
        );
    }
    let request = Request::builder().uri("/").body(Body::empty()).unwrap();
    let response = client.app().oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
}
