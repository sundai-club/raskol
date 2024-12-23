use std::{net::SocketAddr, sync::Arc};

use axum::{
    extract::{Path, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    Json,
};
use dashmap::DashMap;

use crate::{
    auth,
    conf::{Conf, ConfJwt},
};

#[tracing::instrument(name = "server")]
pub async fn run(conf: &Conf) -> anyhow::Result<()> {
    tracing::info!(?conf, "Starting.");
    let addr = SocketAddr::from((conf.addr, conf.port));
    let hits = Hits::new();
    let routes = axum::Router::new()
        .route(
            "/*endpoint",
            axum::routing::post({
                let hits = hits.clone();
                move |endpoint, payload| handle(hits, endpoint, payload)
            })
            .get({
                let hits = hits.clone();
                move || dump(hits)
            }),
        )
        .route_layer(middleware::from_fn({
            // TODO Arc instead of clone.
            let jwt_conf = conf.jwt.clone();
            move |req, next| auth_layer(jwt_conf.clone(), req, next)
        }));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening.");
    axum::serve(listener, routes).await?;
    Ok(())
}

#[tracing::instrument(skip_all)]
async fn handle(
    hits: Hits,
    Path(endpoint): Path<String>,
    payload: Json<Payload>,
) -> Result<Response<String>, StatusCode> {
    let u = USER.get();
    tracing::debug!(user = ?u, ?payload, "Handling.");
    hits.hit(u.uid);
    let url = format!("https://api.groq.com/{endpoint}");
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth("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")
        .json(&payload.0)
        .send()
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to make the external request.");
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    if !resp.status().is_success() {
        tracing::error!(?resp, "External request rejected.");
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    let code = resp.status().as_u16();
    let code = StatusCode::from_u16(code).map_err(|error| {
        tracing::error!(?error, ?code, "Failed to convert status code.");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    let body = resp.text().await.map_err(|error| {
        tracing::error!(?error, ?code, "Failed to receive body from target host.");
        StatusCode::SERVICE_UNAVAILABLE
    })?;
    if is_json(&body) {
        Response::builder()
            .status(code)
            .header(header::CONTENT_TYPE, "application/json")
            .body(body)
    } else {
        Response::builder().status(code).body(body)
    }
    .map_err(|error| {
        tracing::error!(?error, ?code, "Failed to build response.");
        StatusCode::INTERNAL_SERVER_ERROR
    })
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

async fn auth_layer(jwt_conf: ConfJwt, req: Request, next: Next) -> Result<Response, StatusCode> {
    let auth_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(user) = authorize(auth_token, &jwt_conf).await {
        Ok(USER.scope(user, next.run(req)).await)
    } else {
        Err(StatusCode::UNAUTHORIZED)
    }
}

async fn authorize(auth_token: &str, jwt_conf: &ConfJwt) -> Option<User> {
    auth::Claims::from_str(auth_token, jwt_conf)
        .inspect_err(|error| tracing::debug!(?error, "Auth failed."))
        .ok()
        .map(|claims| User { uid: claims.sub })
}

fn is_json(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s).is_ok()
}
