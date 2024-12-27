use std::time::{Duration, SystemTime, UNIX_EPOCH};

use sqlx::Executor;

use crate::conf;

const MIGRATIONS: [&str; 1] = [include_str!("../migrations/0_data.sql")];

#[derive(sqlx::FromRow)]
struct Hit {
    uid: String,
    count_of_all: u64,
    time_of_last: u64,
}

#[derive(Clone)]
pub struct Storage {
    pool: sqlx::Pool<sqlx::Sqlite>,
}

impl Storage {
    pub async fn connect() -> anyhow::Result<Self> {
        let busy_timeout =
            Duration::from_secs_f32(conf::global().sqlite_busy_timeout);
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename("data.db")
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
        let curr = i64::try_from(now.duration_since(UNIX_EPOCH)?.as_secs())?;
        let mut tx = self.pool.begin().await?;
        let prev_opt: Option<Hit> =
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
            Some(Hit {
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
        tx.commit().await?;
        let elapsed_since_prev_hit = now.duration_since(prev_time)?;
        Ok((prev_count.saturating_add(1), elapsed_since_prev_hit))
    }
}
