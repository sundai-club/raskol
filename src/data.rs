use std::{
    fs,
    path::PathBuf,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

use anyhow::Context;
use chrono::{DateTime, Utc};
use sqlx::Executor;

use crate::conf;

const MIGRATIONS: [&str; 1] = [include_str!("../migrations/0_data.sql")];

type Tx<'a> = sqlx::Transaction<'a, sqlx::Sqlite>;

#[derive(sqlx::FromRow)]
struct HitsRow {
    uid: String,
    count_of_all: u64,
    time_of_last: u64,
}

#[derive(sqlx::FromRow)]
struct TokensRow {
    // Used in SQL, but not in Rust. Here for documentation.
    #[allow(dead_code)]
    uid: String,

    // Used in SQL, but not in Rust. Here for documentation.
    #[allow(dead_code)]
    date: String,

    total: u64,
}

#[derive(Clone)]
pub struct Storage {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Storage {
    pub async fn connect() -> anyhow::Result<Self> {
        let file_path = PathBuf::from("data/data.db");
        if let Some(parent) = file_path.parent() {
            let ctx = format!(
                "Failed to create parent directory \
                for database file: {file_path:?}"
            );
            fs::create_dir_all(parent).context(ctx)?;
        }
        let busy_timeout =
            Duration::from_secs_f32(conf::global().sqlite_busy_timeout);
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(file_path)
            .create_if_missing(true)
            .journal_mode(sqlx::sqlite::SqliteJournalMode::Wal)
            .busy_timeout(busy_timeout);
        let pool = sqlx::SqlitePool::connect_with(options).await?;
        let selph = Self { pool };
        for migration in MIGRATIONS {
            selph.pool.execute(migration).await?;
        }
        Ok(selph)
    }

    /// Returns hit count and duration since previous hit.
    pub async fn hit(&self, uid: &str) -> anyhow::Result<(u64, Duration)> {
        let now = SystemTime::now();
        let tx = self.pool.begin().await?;
        let (tx, x) = hit(tx, uid, now).await?;
        tx.commit().await?;
        Ok(x)
    }

    pub async fn tokens_check(
        &self,
        uid: &str,
        requested_amount: usize,
    ) -> anyhow::Result<bool> {
        let requested_amount = u64::try_from(requested_amount)?;
        let now = SystemTime::now();
        let tx = self.pool.begin().await?;
        let (tx, is_enough) =
            tokens_check(tx, uid, now, requested_amount).await?;
        tx.commit().await?;
        Ok(is_enough)
    }

    pub async fn tokens_consume(
        &self,
        uid: &str,
        requested_amount: usize,
    ) -> anyhow::Result<()> {
        let requested_amount = u64::try_from(requested_amount)?;
        let now = SystemTime::now();
        let tx = self.pool.begin().await?;
        let tx = tokens_consume(tx, uid, now, requested_amount).await?;
        tx.commit().await?;
        Ok(())
    }
}

pub async fn hit<'a>(
    mut tx: Tx<'a>,
    uid: &str,
    now: SystemTime,
) -> anyhow::Result<(Tx<'a>, (u64, Duration))> {
    let curr = i64::try_from(now.duration_since(UNIX_EPOCH)?.as_secs())?;
    let prev_opt: Option<HitsRow> =
        sqlx::query_as("SELECT * FROM hits WHERE uid = ?")
            .bind(uid)
            .fetch_optional(&mut *tx)
            .await?;
    let (prev_count, prev_time) = match prev_opt {
        None => {
            sqlx::query(
                "INSERT INTO hits (uid, count_of_all, time_of_last)
                    VALUES (?, 1, ?)",
            )
            .bind(uid)
            .bind(curr)
            .execute(&mut *tx)
            .await?;
            (0, UNIX_EPOCH)
        }
        Some(HitsRow {
            uid,
            count_of_all,
            time_of_last: prev,
        }) => {
            sqlx::query(
                "UPDATE hits SET
                    count_of_all = count_of_all + 1,
                    time_of_last = ?
                    WHERE uid = ?",
            )
            .bind(curr)
            .bind(uid)
            .execute(&mut *tx)
            .await?;
            (count_of_all, UNIX_EPOCH + Duration::from_secs(prev))
        }
    };
    let elapsed_since_prev_hit = now.duration_since(prev_time)?;
    Ok((tx, (prev_count.saturating_add(1), elapsed_since_prev_hit)))
}

async fn tokens_check<'a>(
    mut tx: Tx<'a>,
    uid: &str,
    now: SystemTime,
    requested_amount: u64,
) -> anyhow::Result<(Tx<'a>, bool)> {
    let date = DateTime::<Utc>::from(now).format("%Y-%m-%d").to_string();
    let prev_opt: Option<TokensRow> =
        sqlx::query_as("SELECT * FROM tokens WHERE uid = ? AND date = ?")
            .bind(uid)
            .bind(&date)
            .fetch_optional(&mut *tx)
            .await?;
    let used = match prev_opt {
        None => {
            sqlx::query(
                "INSERT INTO tokens (uid, date, total)
                    VALUES (?, ?, 0)",
            )
            .bind(uid)
            .bind(&date)
            .execute(&mut *tx)
            .await?;
            0
        }
        Some(TokensRow {
            uid: _,
            date: _,
            total,
        }) => total,
    };
    let max = conf::global().max_tokens_per_day;
    let remaining = max.saturating_sub(used);
    Ok((tx, remaining >= requested_amount))
}

async fn tokens_consume<'a>(
    mut tx: Tx<'a>,
    uid: &str,
    now: SystemTime,
    requested_amount: u64,
) -> anyhow::Result<Tx<'a>> {
    let date = DateTime::<Utc>::from(now).format("%Y-%m-%d").to_string();
    let requested_amount = i64::try_from(requested_amount)?;
    sqlx::query(
        "INSERT INTO tokens (uid, date, total)
                    VALUES (?, ?, ?)
                    ON CONFLICT(uid, date) DO UPDATE SET
                    total = total + ?
                    ",
    )
    .bind(uid)
    .bind(&date)
    .bind(requested_amount)
    .bind(requested_amount)
    .execute(&mut *tx)
    .await?;
    Ok(tx)
}
