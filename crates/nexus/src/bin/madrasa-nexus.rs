//! The Madrasa Nexus hub server.
//!
//! Environment:
//! - `NEXUS_DATABASE_URL` PostgreSQL URL of the hub database (required)
//! - `NEXUS_BIND` address to bind, default `0.0.0.0:4780`; or `PORT` (Cloud Run, Container Apps)
//! - `NEXUS_PUBLIC_URL` the address schools use, shown in invitations
//! - `NEXUS_SETUP_TOKEN` one-time token for first-run setup (generated and printed if unset)
//! - `RUST_LOG` tracing filter, default `info`
//!
//! `madrasa-nexus healthcheck` exits 0 when the local hub answers `/health` (used by the
//! container health check).
//!
//! `madrasa-nexus demo-network --yes` fills an empty hub with three made-up schools for
//! trying it out, then exits.

use madrasa_nexus::{server, store, Config, HubState};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("healthcheck") {
        let port = std::env::var("PORT")
            .ok()
            .and_then(|p| p.trim().parse().ok())
            .or_else(|| {
                std::env::var("NEXUS_BIND")
                    .ok()
                    .and_then(|b| b.trim().parse::<std::net::SocketAddr>().ok())
                    .map(|a| a.port())
            })
            .unwrap_or(madrasa_nexus::config::DEFAULT_PORT);
        std::process::exit(if server::healthcheck(port) { 0 } else { 1 });
    }

    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "info".into()),
        )
        // Container logs are read as plain text: colour codes only when a person is watching.
        .with_ansi(std::io::IsTerminal::is_terminal(&std::io::stdout()))
        .init();

    let config = Config::from_env()?;
    // sqlx's errors can quote the URL; say what failed without it.
    let pool = store::connect(&config.database_url).await.map_err(|e| {
        anyhow::anyhow!(
            "cannot connect to the hub database: {}",
            redact(&e.to_string())
        )
    })?;
    store::migrate(&pool).await?;
    if std::env::args().nth(1).as_deref() == Some("demo-network") {
        // A refusal is something to read, not a crash: one plain line, exit code 1.
        if let Err(problem) = demo_network(&pool).await {
            eprintln!("{problem}");
            std::process::exit(1);
        }
        return Ok(());
    }
    // After an upgrade, bring every school's schema to this version.
    let schools = madrasa_nexus::tenants::migrate_all(&pool).await?;
    tracing::info!(schools, "school schemas are up to date");
    let (state, generated_token) = HubState::new(pool, &config).await?;
    if let Some(token) = generated_token {
        // Printed once, on purpose: the first administrator needs it to finish setup. It
        // stops working as soon as setup is done. Set NEXUS_SETUP_TOKEN to choose your own.
        tracing::warn!("first-run setup is open; setup token: {token}");
    }
    let (addr, fut) = server::bind(config.bind, state, shutdown_signal()).await?;
    tracing::info!(%addr, version = madrasa_nexus::VERSION, "madrasa-nexus listening");
    fut.await?;
    Ok(())
}

/// `madrasa-nexus demo-network --yes`: three made-up schools in an empty hub.
async fn demo_network(pool: &sqlx::PgPool) -> anyhow::Result<()> {
    if !std::env::args().any(|a| a == "--yes") {
        anyhow::bail!(
            "This fills the hub with made-up demo schools. It is for a hub you are only trying out. Run it again with --yes to go ahead."
        );
    }
    let node = {
        let mut conn = pool.acquire().await?;
        store::node_id(&mut conn).await?
    };
    let hlc = madrasa_core::hlc::Hlc::new(node);
    let schools = madrasa_nexus::demo::seed_network(pool, &hlc).await?;
    for school in &schools {
        println!("created {} ({})", school.name, school.code);
    }
    println!(
        "The demo network is ready: {} schools. Start the hub and open its address.",
        schools.len()
    );
    Ok(())
}

/// Strip anything that looks like a connection URL from an error message.
fn redact(message: &str) -> String {
    message
        .split_whitespace()
        .map(|w| if w.contains("://") { "<url>" } else { w })
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve on Ctrl+C or, on Unix, SIGTERM (what `docker stop` sends).
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
            }
            Err(_) => std::future::pending::<()>().await,
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();
    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
}
