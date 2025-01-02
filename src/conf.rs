use std::{
    fmt::Debug,
    fs,
    net::IpAddr,
    path::{Path, PathBuf},
    str::FromStr,
    sync::{Arc, LazyLock},
};

use anyhow::Context;

pub static GLOBAL: LazyLock<Arc<Conf>> = LazyLock::new(|| {
    let conf = read_or_create_default().unwrap_or_else(|error| {
        panic!("Failed to initialize global config: {error:?}")
    });
    Arc::new(conf)
});

#[must_use]
pub fn global() -> Arc<Conf> {
    (*GLOBAL).clone()
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Conf {
    #[serde(
        serialize_with = "serialize_log_level",
        deserialize_with = "deserialize_log_level"
    )]
    pub log_level: tracing::Level,
    pub addr: IpAddr,
    pub port: u16,
    pub jwt: ConfJwt,
    pub target_address: String,
    pub target_auth_token: String,
    pub min_hit_interval: f32,
    pub max_tokens_per_day: u64,
    pub sqlite_busy_timeout: f32,
    pub tls: Option<Tls>,
}

impl Default for Conf {
    fn default() -> Self {
        Self {
            log_level: tracing::Level::INFO,
            addr: "127.0.0.1".parse().unwrap_or_else(|_| {
                unreachable!("Fat-fingered default IP address!")
            }),
            port: 3001,
            jwt: ConfJwt::default(),
            target_address: "api.groq.com".to_string(),
            target_auth_token: String::new(),
            min_hit_interval: 5.0,
            max_tokens_per_day: 1_000_000, // TODO Revise.
            sqlite_busy_timeout: 60.0,
            tls: None,
        }
    }
}

#[derive(serde::Serialize, serde::Deserialize, Debug, Clone)]
pub struct Tls {
    pub cert_file: PathBuf,
    pub key_file: PathBuf,
}

#[derive(serde::Serialize, serde::Deserialize, Clone)]
pub struct ConfJwt {
    pub secret: String,
    pub audience: String,
    pub issuer: String,
}

impl Default for ConfJwt {
    fn default() -> Self {
        Self {
            secret: "super-secret".to_string(),
            audience: "authenticated".to_string(),
            issuer: "https://bright-kitten-41.clerk.accounts.dev".to_string(),
        }
    }
}

impl Debug for ConfJwt {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfJwt")
            .field("secret", &"<XXXXX>")
            .field("audience", &self.audience)
            .field("issuer", &self.issuer)
            .finish()
    }
}

fn serialize_log_level<S>(
    level: &tracing::Level,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let s = level.to_string();
    serializer.serialize_str(&s)
}

fn deserialize_log_level<'de, D>(
    deserializer: D,
) -> Result<tracing::Level, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize;

    let s = String::deserialize(deserializer)?;
    tracing::Level::from_str(&s).map_err(serde::de::Error::custom)
}

pub fn read_or_create_default() -> anyhow::Result<Conf> {
    let path = "conf/conf.toml";
    read_or_create_default_(path).context(path)
}

pub fn read_or_create_default_<P: AsRef<Path>>(
    path: P,
) -> anyhow::Result<Conf> {
    let path = path.as_ref();
    let conf = if fs::exists(path)? {
        let s = fs::read_to_string(path)?;
        toml::from_str(&s)?
    } else {
        if let Some(parent) = path.parent() {
            let ctx = format!(
                "Failed to create parent directory \
                for conf file: {path:?}"
            );
            fs::create_dir_all(parent).context(ctx)?;
        }
        let conf = Conf::default();
        let s = toml::to_string_pretty(&conf)?;
        fs::write(path, s)?;
        conf
    };
    Ok(conf)
}
