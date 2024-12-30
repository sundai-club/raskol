use std::{
    env, fs,
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::Context;
use clap::Parser;

#[derive(Parser, Debug)]
struct Cli {
    /// Working directory, with config and data files.
    #[clap(short, long, default_value = "data")]
    dir: PathBuf,

    #[clap(subcommand)]
    cmd: Cmd,
}

#[derive(clap::Subcommand, Debug)]
enum Cmd {
    Server,
    Jwt { uid: String, ttl: f64 },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    human_panic_setup();
    let cli = Cli::parse();
    set_current_dir(&cli.dir)?;
    raskol::tracing::init()?;
    tracing::debug!(?cli, "Starting.");
    match &cli.cmd {
        Cmd::Server => raskol::server::run().await,
        Cmd::Jwt { uid, ttl } => {
            let conf = raskol::conf::global();
            let claims = raskol::auth::Claims::new(
                uid,
                Duration::from_secs_f64(*ttl),
            )?;
            let encoded: String = claims.to_str(&conf.jwt)?;
            println!("{encoded}");
            Ok(())
        }
    }
}

fn set_current_dir(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path)
        .context(format!("Failed to create directory path: {path:?}"))?;
    env::set_current_dir(path)
        .context(format!("Failed to set current directory to {path:?}"))?;
    Ok(())
}

fn human_panic_setup() {
    macro_rules! repo {
        () => {
            env!("CARGO_PKG_REPOSITORY")
        };
    }
    human_panic::setup_panic!(human_panic::Metadata::new(
        env!("CARGO_PKG_NAME"),
        env!("CARGO_PKG_VERSION")
    )
    .authors(env!("CARGO_PKG_AUTHORS"))
    .homepage(repo!())
    .support(concat!("- Submit an issue at ", repo!(), "/issues")));
}
