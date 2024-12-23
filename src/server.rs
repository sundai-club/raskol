use std::{net::SocketAddr, sync::Arc, time::{Duration, Instant}};

use axum::{
    extract::{Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    Json,
};
use dashmap::DashMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::{auth, jwt};

#[derive(Debug, Serialize, Deserialize)]
struct Payload {
    messages: Vec<Message>,
    model: Option<String>,
    temperature: Option<f32>,
    max_tokens: Option<u32>,
}

#[derive(Debug, Serialize, Deserialize)]
struct Message {
    role: String,
    content: String,
}

#[derive(Clone)]
struct GroqConfig {
    api_key: String,
    base_url: String,
}

impl GroqConfig {
    fn new(api_key: String) -> Self {
        Self {
            api_key,
            base_url: "https://api.groq.com/openai/v1/chat/completions".to_string(),
        }
    }
}

#[tracing::instrument(name = "server")]
pub async fn run(addr: SocketAddr, jwt_opts: &jwt::Options, groq_api_key: String) -> anyhow::Result<()> {
    let hits = Hits::new();
    let groq_config = GroqConfig::new(groq_api_key);
    
    tracing::info!("Starting.");
    let routes = axum::Router::new()
        .route(
            "/api/chat",
            axum::routing::post(handle_chat),
        )
        .route_layer(middleware::from_fn({
            let jwt_opts = jwt_opts.clone();
            move |req, next| auth_layer(jwt_opts.clone(), req, next)
        }))
        .with_state((hits, groq_config));

    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening.");
    axum::serve(listener, routes).await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn handle_chat(
    State((hits, config)): State<(Hits, GroqConfig)>,
    payload: Json<Payload>,
) -> Result<Json<Value>, StatusCode> {
    let u = USER.get();
    tracing::debug!(user = ?u, "Handling chat request.");
    
    hits.hit(u.uid)?;
    
    let groq_payload = serde_json::json!({
        "messages": payload.messages,
        "model": payload.model.as_deref().unwrap_or("mixtral-8x7b-32768"),
        "temperature": payload.temperature.unwrap_or(0.7),
        "max_tokens": payload.max_tokens.unwrap_or(2048),
    });

    let response = reqwest::Client::new()
        .post(&config.base_url)
        .header("Authorization", format!("Bearer {}", config.api_key))
        .json(&groq_payload)
        .send()
        .await
        .map_err(|e| {
            tracing::error!("Groq API request failed: {}", e);
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    let groq_response = response.json::<Value>().await.map_err(|e| {
        tracing::error!("Failed to parse Groq response: {}", e);
        StatusCode::INTERNAL_SERVER_ERROR
    })?;

    Ok(Json(groq_response))
}

#[derive(Clone, Debug)]
struct Hits {
    per_user: Arc<DashMap<String, usize>>,
    rate_limits: Arc<DashMap<String, Instant>>,
}

impl Hits {
    fn new() -> Self {
        Self {
            per_user: Arc::new(DashMap::new()),
            rate_limits: Arc::new(DashMap::new()),
        }
    }

    fn check_rate_limit(&self, uid: &str) -> bool {
        const RATE_LIMIT_DURATION: Duration = Duration::from_secs(60); // 1 request per minute
        
        let now = Instant::now();
        if let Some(last_hit) = self.rate_limits.get(uid) {
            if now.duration_since(*last_hit) < RATE_LIMIT_DURATION {
                return false;
            }
        }
        self.rate_limits.insert(uid.to_string(), now);
        true
    }

    fn hit(&self, uid: String) -> Result<(), StatusCode> {
        if !self.check_rate_limit(&uid) {
            return Err(StatusCode::TOO_MANY_REQUESTS);
        }
        
        self.per_user
            .entry(uid)
            .and_modify(|count| *count += 1)
            .or_insert(1);
        Ok(())
    }

    #[allow(dead_code)]
    fn get(&self, uid: &str) -> Option<usize> {
        self.per_user.get(uid).map(|count| *count)
    }
}

#[derive(Debug, Clone)]
struct User {
    pub uid: String,
}

tokio::task_local! {
    pub static USER: User;
}

async fn auth_layer(
    jwt_opts: jwt::Options,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .and_then(|auth_str| {
            // Extract token from "Bearer <token>"
            if auth_str.starts_with("Bearer ") {
                Some(auth_str[7..].to_string())
            } else {
                tracing::debug!("Invalid Authorization header format");
                None
            }
        })
        .ok_or(StatusCode::UNAUTHORIZED)?;

    if let Some(user) = authorize(&auth_token, &jwt_opts).await {
        Ok(USER.scope(user, next.run(req)).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn authorize(auth_token: &str, jwt_opts: &jwt::Options) -> Option<User> {
    auth::Claims::from_str(auth_token, jwt_opts)
        .inspect_err(|error| tracing::debug!(?error, "Auth failed."))
        .ok()
        .map(|claims| User { uid: claims.sub })
}

#[cfg(test)]
mod tests;
