//! `madrasa-demo`: generate or load the DEMO school. Never runs on its own.
//!
//! ```text
//! madrasa-demo generate --out <dir>            write JSON files only
//! madrasa-demo load --yes [--out <dir>]        insert into MADRASA_DATABASE_URL (refuses if a
//!                                              real school exists)
//! ```

use madrasa_core::hlc::Hlc;
use madrasa_demo_data::{generate, load, write_json, DemoOptions, LoadOptions, DEMO_PASSWORD};
use std::path::PathBuf;

fn usage() -> ! {
    eprintln!(
        "usage:\n  madrasa-demo generate --out <dir>\n  madrasa-demo load --yes [--out <dir>]"
    );
    std::process::exit(2)
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt().with_env_filter("info").init();
    let args: Vec<String> = std::env::args().skip(1).collect();
    let command = args.first().map(String::as_str).unwrap_or("");
    let mut out: Option<PathBuf> = None;
    let mut yes = false;
    let mut i = 1;
    while i < args.len() {
        match args[i].as_str() {
            "--out" => {
                i += 1;
                out = Some(PathBuf::from(
                    args.get(i).cloned().unwrap_or_else(|| usage()),
                ));
            }
            "--yes" => yes = true,
            _ => usage(),
        }
        i += 1;
    }

    let demo = generate(&DemoOptions::default());
    match command {
        "generate" => {
            let dir = out.unwrap_or_else(|| usage());
            let files = write_json(&dir, &demo)?;
            println!("wrote {} files to {}", files.len(), dir.display());
        }
        "load" => {
            let url = std::env::var("MADRASA_DATABASE_URL")
                .map_err(|_| anyhow::anyhow!("MADRASA_DATABASE_URL is required"))?;
            let pool = madrasa_db::connect(&url).await?;
            madrasa_db::migrate(&pool).await?;
            let node = {
                let mut conn = pool.acquire().await?;
                madrasa_db::settings::node_id(&mut conn).await?
            };
            let report = load(
                &pool,
                &Hlc::new(node),
                &demo,
                &LoadOptions {
                    confirmed: yes,
                    out_dir: out,
                },
            )
            .await?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            println!("demo accounts use the password: {DEMO_PASSWORD}");
        }
        _ => usage(),
    }
    Ok(())
}
