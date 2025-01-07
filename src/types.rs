#[derive(serde::Serialize)]
pub struct UserStats {
    pub uid: String,
    pub total_hits: u64,
    pub last_hit_time: u64,
    pub tokens_used_today: u64,
} 