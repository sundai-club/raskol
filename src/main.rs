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
    let cli = Cli::parse();
    set_current_dir(&cli.dir)?;
    let conf = raskol::conf::read_or_create_default()?;
    raskol::tracing::init(conf.log_level)?;
    tracing::debug!(?cli, ?conf, "Starting.");
    match &cli.cmd {
        Cmd::Server => raskol::server::run(&conf).await,
        Cmd::Jwt { uid, ttl } => {
            let claims = raskol::auth::Claims::new(uid, Duration::from_secs_f64(*ttl))?;
            let encoded: String = claims.to_str(&conf.jwt)?;
            println!("{encoded}");
            Ok(())
        }
    }
}

fn set_current_dir(path: &Path) -> anyhow::Result<()> {
    fs::create_dir_all(path).context(format!("Failed to create directory path: {path:?}"))?;
    env::set_current_dir(&path).context(format!("Failed to set current directory to {path:?}"))?;
    Ok(())
}
