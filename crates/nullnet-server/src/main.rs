//! ```text
//! cargo run -p nullnet-server -- --db nullnet.db --port 8080 --web dist
//! ```
//!
//! Then open http://localhost:8080/console to create a game.

use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Duration;

use nullnet_server::{Server, notify, now, router};

struct Options {
    db: PathBuf,
    port: u16,
    web: Option<PathBuf>,
}

/// Defaults come from the environment where a host sets them (`PORT`,
/// `NULLNET_DB`, `NULLNET_WEB`); the arguments override them.
fn parse(mut args: impl Iterator<Item = String>) -> Result<Options, String> {
    let env = |name: &str| std::env::var(name).ok().filter(|v| !v.is_empty());
    let mut options = Options {
        db: env("NULLNET_DB").map_or_else(|| PathBuf::from("nullnet.db"), PathBuf::from),
        port: match env("PORT") {
            Some(port) => port.parse().map_err(|e| format!("PORT: {e}"))?,
            None => 8080,
        },
        web: env("NULLNET_WEB").map(PathBuf::from),
    };
    while let Some(arg) = args.next() {
        let mut value = || args.next().ok_or(format!("{arg} needs a value"));
        match arg.as_str() {
            "--db" => options.db = PathBuf::from(value()?),
            "--port" => options.port = value()?.parse().map_err(|e| format!("--port: {e}"))?,
            "--web" => options.web = Some(PathBuf::from(value()?)),
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    Ok(options)
}

/// How often turns whose deadline has passed are run.
const TICK: Duration = Duration::from_secs(30);

#[tokio::main]
async fn main() -> ExitCode {
    let options = match parse(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(error) => {
            eprintln!("{error}");
            eprintln!(
                "usage: nullnet-server [--db FILE] [--port N] [--web DIR] (or NULLNET_DB, PORT, NULLNET_WEB)"
            );
            return ExitCode::FAILURE;
        }
    };
    let server = match Server::open(&options.db) {
        Ok(server) => Arc::new(server),
        Err(error) => {
            eprintln!("cannot open {}: {error}", options.db.display());
            return ExitCode::FAILURE;
        }
    };

    let ticker = Arc::clone(&server);
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(TICK);
        loop {
            interval.tick().await;
            match ticker.resolve_due(now()) {
                Ok(0) => {}
                Ok(ran) => println!("ran {ran} turn(s) whose deadline had passed"),
                Err(error) => eprintln!("deadline check failed: {error}"),
            }
            let notices = ticker.take_notices();
            if !notices.is_empty() {
                notify::deliver(notices).await;
            }
        }
    });

    let app = router(server, options.web.clone());
    let address = format!("0.0.0.0:{}", options.port);
    let listener = match tokio::net::TcpListener::bind(&address).await {
        Ok(listener) => listener,
        Err(error) => {
            eprintln!("cannot listen on {address}: {error}");
            return ExitCode::FAILURE;
        }
    };
    println!(
        "NullNet server on http://localhost:{}/console ({}{})",
        options.port,
        options.db.display(),
        options
            .web
            .as_ref()
            .map(|w| format!(", client from {}", w.display()))
            .unwrap_or_default()
    );
    let served = axum::serve(listener, app).with_graceful_shutdown(async {
        let _ = tokio::signal::ctrl_c().await;
    });
    match served.await {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("server stopped: {error}");
            ExitCode::FAILURE
        }
    }
}
