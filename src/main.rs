use std::{
    fs,
    net::{IpAddr, SocketAddr},
    path::PathBuf,
    time::Duration,
};

use clap::Parser;
use raskol::jwt;

#[derive(Parser, Debug)]
struct Cli {
    #[clap(short, long, default_value = "127.0.0.1")]
    addr: IpAddr,

    #[clap(short, long, default_value = "3000")]
    port: u16,

    #[clap(short, long = "log", default_value_t = tracing::Level::DEBUG)]
    log_level: tracing::Level,

    #[clap(long, default_value = "jwt-secret.txt")]
    jwt_secret_file: PathBuf,

    #[clap(long, default_value = "authenticated")]
    jwt_audience: String,

    #[clap(long, default_value = "https://bright-kitten-41.clerk.accounts.dev")]
    jwt_issuer: String,

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
    raskol::tracing::init(cli.log_level)?;
    tracing::debug!(?cli, "Starting.");
    let addr = SocketAddr::from((cli.addr, cli.port));
    let jwt_secret = fs::read_to_string(cli.jwt_secret_file)?.trim().to_string();
    let jwt_opts = jwt::Options {
        secret: jwt_secret.to_string(),
        audience: cli.jwt_audience.to_string(),
        issuer: cli.jwt_issuer.to_string(),
    };
    match &cli.cmd {
        Cmd::Server => raskol::server::run(addr, &jwt_opts).await,
        Cmd::Jwt { uid, ttl } => {
            let claims = raskol::auth::Claims::new(uid, Duration::from_secs_f64(*ttl))?;
            let encoded: String = claims.to_str(&jwt_opts)?;
            println!("{encoded}");
            Ok(())
        }
    }
}
