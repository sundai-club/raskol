use utoipa::ToSchema;

#[derive(serde::Serialize, ToSchema)]
pub struct UserStats {
    pub uid: String,
    pub total_hits: u64,
    pub last_hit_time: u64,
    pub tokens_used_today: u64,
} 