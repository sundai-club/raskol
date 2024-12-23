use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::Request,
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    Json,
};
use dashmap::DashMap;

use crate::{auth, jwt};

#[tracing::instrument(name = "server")]
pub async fn run(addr: SocketAddr, jwt_opts: &jwt::Options) -> anyhow::Result<()> {
    let hits = Hits::new();
    tracing::info!("Starting.");
    let routes = axum::Router::new()
        .route(
            "/api",
            axum::routing::post({
                let hits = hits.clone();
                move |payload| handle(hits, payload)
            })
            .get({
                let hits = hits.clone();
                move || dump(hits)
            }),
        )
        .route_layer(middleware::from_fn({
            let jwt_opts = jwt_opts.clone();
            move |req, next| auth_layer(jwt_opts.clone(), req, next)
        }));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening.");
    axum::serve(listener, routes).await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn handle(hits: Hits, payload: Json<Payload>) -> Result<StatusCode> {
    let u = USER.get();
    tracing::debug!(user = ?u, ?payload, "Handling.");
    hits.hit(u.uid);
    let resp_result = reqwest::Client::new()
        .post("https://api.groq.com/openai/v1/chat/completions")
        .json(&payload.0)
        .send()
        .await;
    // TODO Revise status codes.
    match resp_result {
        Ok(resp) => {
            let code = resp.status().as_u16();
            let code = StatusCode::from_u16(code).unwrap_or_else(|error| {
                tracing::error!(?error, ?code, "Failed to convert status code.");
                StatusCode::INTERNAL_SERVER_ERROR
            });
            Ok(code)
        }
        Err(error) => {
            tracing::error!(?error, "Failed to make the external request.");
            Ok(StatusCode::SERVICE_UNAVAILABLE)
        }
    }
}

#[tracing::instrument(skip_all)]
async fn dump(hits: Hits) -> Result<Json<Option<usize>>, StatusCode> {
    let u = USER.get();
    tracing::debug!(user = ?u, "Dumping.");
    Ok(Json(hits.get(&u.uid)))
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Payload {
    messages: Vec<Msg>,
    model: String,
}

#[derive(Debug, serde::Serialize, serde::Deserialize)]
struct Msg {
    role: String,
    content: String,
}

#[derive(Clone, Debug, Default)]
struct Hits {
    per_user: Arc<DashMap<String, usize>>,
}

impl Hits {
    fn new() -> Self {
        Self::default()
    }

    fn hit(&self, uid: String) {
        self.per_user
            .entry(uid)
            .and_modify(|count| *count += 1)
            .or_insert(1);
    }

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
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(user) = authorize(auth_token, &jwt_opts).await {
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
