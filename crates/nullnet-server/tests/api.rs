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
                    "difficulty": "normal",
                    "last_turn": 50,
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

/// A command that does nothing wrong, for a hand-in that should be accepted.
fn scan(hacker: u32, host: u32) -> Value {
    json!({ "Scan": { "hacker": hacker, "host": host } })
}

#[tokio::test]
async fn a_game_hands_out_one_invite_per_person_and_none_to_bots() {
    let client = Client::in_memory();
    let created = client
        .create(
            json!([{ "name": "Ghostline" }, { "name": "Blackice", "bot": true }, { "name": "Nullset" }]),
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
    assert_eq!(created["difficulty"], "Normal");
    assert_eq!(created["last_turn"], 50);

    let (status, me) = client
        .get(&format!("/api/crew/{}", token(&created, 0)))
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(me["name"], "Ghostline");
    assert_eq!(me["turn"], 1, "the game opens on turn 1");
    assert_eq!(me["submitted"], false);
    assert_eq!(me["crews"][1]["submitted"], true, "a bot is always ready");
    assert_eq!(me["view"]["me"]["hackers"].as_array().unwrap().len(), 2);
    assert!(me["last_turn"].is_null());

    let (status, _) = client.get("/api/crew/not-a-token").await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn games_need_a_name_a_person_and_sensible_settings() {
    let client = Client::in_memory();
    for body in [
        json!({ "name": " ", "crews": [{ "name": "A" }] }),
        json!({ "name": "x", "crews": [] }),
        json!({ "name": "x", "crews": [{ "name": "A", "bot": true }] }),
        json!({ "name": "x", "crews": [{ "name": "A" }, { "name": "B" }, { "name": "C" }, { "name": "D" }, { "name": "E" }] }),
        json!({ "name": "x", "crews": [{ "name": "A" }], "last_turn": 2 }),
        json!({ "name": "x", "crews": [{ "name": "A" }], "difficulty": "brutal" }),
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

    // Ghostline scans two hosts near its hideout.
    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    let reach: Vec<u64> = me["view"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["can_scan"].as_bool().unwrap())
        .map(|h| h["host"].as_u64().unwrap())
        .collect();
    let (status, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{a}/orders"),
            Some(json!([scan(0, reach[0] as u32)])),
        )
        .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["rejected"].as_array().unwrap().len(), 0);
    assert_eq!(receipt["resolved"], false);

    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["submitted"], true);
    assert_eq!(me["orders"].as_array().unwrap().len(), 1);
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
            Some(json!([])),
        )
        .await;
    assert_eq!(receipt["resolved"], true);

    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    assert_eq!(me["turn"], 2);
    assert_eq!(me["submitted"], false);
    assert_eq!(me["last_turn"]["turn"], 1);
    assert_eq!(me["last_turn"]["orders"].as_array().unwrap().len(), 1);
    assert!(
        me["last_turn"]["report"]["events"]
            .as_array()
            .unwrap()
            .iter()
            .any(|e| e.get("Scanned").is_some()),
        "the scan happened"
    );

    let (_, turns) = client.get(&format!("/api/crew/{a}/turns")).await;
    assert_eq!(turns.as_array().unwrap().len(), 1);
    assert_eq!(turns[0]["turn"], 1);
    let (status, turn) = client.get(&format!("/api/crew/{b}/turns/1")).await;
    assert_eq!(status, StatusCode::OK);
    assert!(
        turn["orders"].as_array().unwrap().is_empty(),
        "Blackice handed in nothing"
    );
    let (status, _) = client.get(&format!("/api/crew/{b}/turns/9")).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn a_practice_game_runs_each_turn_at_once() {
    let client = Client::in_memory();
    let (status, created) = client
        .call(
            Method::POST,
            "/api/games",
            Some(json!({
                "name": "Practice",
                "crews": [{ "name": "Solo" }, { "name": "Bot", "bot": true }],
                "difficulty": "easy",
                "last_turn": 5,
                "deadline_hours": 0,
                "seed": 7
            })),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let me = token(&created, 0);

    for turn in 1..=5 {
        let (status, me_view) = client.get(&format!("/api/crew/{me}")).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(me_view["turn"], turn);
        assert_eq!(me_view["over"], false);
        let (_, receipt) = client
            .call(
                Method::PUT,
                &format!("/api/crew/{me}/orders"),
                Some(json!([])),
            )
            .await;
        assert_eq!(receipt["resolved"], true, "a practice turn runs at once");
    }

    let (_, me_view) = client.get(&format!("/api/crew/{me}")).await;
    assert_eq!(me_view["over"], true);
    assert!(me_view["view"]["over"].as_bool().unwrap());
    assert_eq!(me_view["view"]["scores"].as_array().unwrap().len(), 2);

    // No more orders once the game is over.
    let (status, _) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{me}/orders"),
            Some(json!([])),
        )
        .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
}

#[tokio::test]
async fn a_turn_that_ran_is_posted_to_the_game_webhook() {
    use axum::Json;
    use axum::routing::post;
    use std::sync::Mutex;

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
                "last_turn": 5,
                "deadline_hours": 0,
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
    assert!(text.contains("turn 1"), "{text}");

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
    let (_, me) = client.get(&format!("/api/crew/{a}")).await;
    let hideout = me["view"]["me"]["hideout"].as_u64().unwrap() as u32;
    // Scanning the hideout is refused: a crew already knows its own hosts.
    let (_, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{a}/orders"),
            Some(json!([scan(0, hideout), { "BuyZeroDay": null }])),
        )
        .await;
    assert_eq!(receipt["rejected"][0]["index"], 0);
    assert_eq!(
        receipt["rejected"][0]["error"],
        "you already know that host"
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
    assert_eq!(me["turn"], 2);
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
    assert_eq!(me["turn"], 31);
    // The bot has taken hosts; the person has done nothing.
    let bot = me["view"]["crews"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["player"] == 1)
        .unwrap();
    assert!(bot["hosts"].as_u64().unwrap() >= 1, "the bot took hosts");
    let my_hosts = me["view"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|h| h["controller"] == json!({ "Crew": 0 }))
        .count();
    assert_eq!(my_hosts, 1, "the idle crew holds only its hideout");
    assert_eq!(client.server.turns(&a).unwrap().len(), 30);
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
        let (_, me) = client.get(&format!("/api/crew/{a}")).await;
        let host = me["view"]["hosts"]
            .as_array()
            .unwrap()
            .iter()
            .find(|h| h["can_scan"].as_bool().unwrap())
            .unwrap()["host"]
            .as_u64()
            .unwrap() as u32;
        client
            .call(
                Method::PUT,
                &format!("/api/crew/{a}/orders"),
                Some(json!([scan(0, host)])),
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
    assert!(me["orders"][0]["Scan"].is_object());
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

#[tokio::test]
async fn a_game_can_be_played_on_a_random_map() {
    let client = Client::in_memory();
    let game = |map: Value, hosts: Value| {
        json!({
            "name": "Somewhere new",
            "crews": [{ "name": "Solo" }, { "name": "Bot", "bot": true }],
            "deadline_hours": 0,
            "seed": 11,
            "map": map,
            "hosts": hosts
        })
    };
    for (map, hosts) in [
        (json!("random"), json!(5)),
        (json!("random"), json!(500)),
        (json!("maze"), json!(41)),
    ] {
        let (status, _) = client
            .call(Method::POST, "/api/games", Some(game(map, hosts)))
            .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    let (status, created) = client
        .call(
            Method::POST,
            "/api/games",
            Some(game(json!("random"), json!(60))),
        )
        .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    assert_eq!(
        created["map"]["Random"]["hosts"], 61,
        "rounded to four corners and Cortex"
    );
    let me = token(&created, 0);

    let (_, status) = client.get(&format!("/api/crew/{me}")).await;
    assert_eq!(status["view"]["map"]["Random"]["hosts"], 61);
    assert_eq!(status["view"]["hosts"].as_array().unwrap().len(), 61);

    // The crew's map is the game's own, not the standard one.
    let (code, map) = client.get(&format!("/api/crew/{me}/map")).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(map["hosts"].as_array().unwrap().len(), 61);
    let (_, standard) = client.get("/api/map").await;
    assert_eq!(standard["hosts"].as_array().unwrap().len(), 41);

    // Turns run on it like on any other map.
    let target = status["view"]["hosts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|h| h["can_scan"] == true)
        .unwrap()["host"]
        .as_u64()
        .unwrap() as u32;
    let hacker = status["view"]["me"]["hackers"][0]["id"].as_u64().unwrap() as u32;
    let (_, receipt) = client
        .call(
            Method::PUT,
            &format!("/api/crew/{me}/orders"),
            Some(json!([scan(hacker, target)])),
        )
        .await;
    assert_eq!(receipt["rejected"], json!([]));
    assert_eq!(receipt["resolved"], true);
    let (_, status) = client.get(&format!("/api/crew/{me}")).await;
    assert_eq!(status["turn"], 2);
    assert!(!status["view"]["hosts"][target as usize]["intel"].is_null());
}
