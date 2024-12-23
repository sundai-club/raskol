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

#[tracing::instrument(name = "server", skip_all, fields(addr = ?conf.addr, port = conf.port))]
pub async fn run(conf: &Conf) -> anyhow::Result<()> {
    tracing::info!(?conf, "Starting.");
    let addr = SocketAddr::from((conf.addr, conf.port));
    let hits = Hits::new();
    let jwt_conf = Arc::new(conf.jwt.clone());
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
            move |req, next: Next| REQ_ID.scope(ReqId::new(), next.run(req))
        }))
        .route_layer(middleware::from_fn({
            move |req, next: Next| auth_layer(jwt_conf.clone(), req, next)
        }));
    let listener = tokio::net::TcpListener::bind(addr).await?;
    tracing::info!("Listening.");
    axum::serve(listener, routes).await?;
    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid
    )
)]
async fn handle(
    hits: Hits,
    Path(endpoint): Path<String>,
    payload: Json<serde_json::Value>,
) -> Result<Response<String>, StatusCode> {
    let user: User = USER.get();
    hits.hit(&user.uid);
    tracing::debug!(hits = hits.get(&user.uid), "Handling.");
    let address = "api.groq.com"; // TODO Move to conf.
    let url = format!("https://{address}/{endpoint}");
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(
            // TODO Move to conf.
            "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",
        )
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
        tracing::error!(
            ?error,
            ?code,
            "Failed to receive body from target host."
        );
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

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid
    )
)]
async fn dump(hits: Hits) -> Result<Json<Option<usize>>, StatusCode> {
    tracing::debug!("Dumping.");
    let u = USER.get();
    Ok(Json(hits.get(&u.uid)))
}

#[derive(Clone, Debug, Default)]
struct Hits {
    per_user: Arc<DashMap<String, usize>>,
}

impl Hits {
    fn new() -> Self {
        Self::default()
    }

    fn hit(&self, uid: &str) {
        self.per_user
            .entry(uid.to_string())
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

#[derive(Debug, Clone)]
struct ReqId {
    pub req_id: String,
}

impl ReqId {
    fn new() -> Self {
        let req_id = cuid2::create_id();
        Self { req_id }
    }
}

tokio::task_local! {
    pub static USER: User;
    pub static REQ_ID: ReqId;
}

async fn auth_layer(
    jwt_conf: Arc<ConfJwt>,
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let auth_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(user) = authorize(auth_token, &jwt_conf).await {
        Ok(USER.scope(user, next.run(req)).await)
    } else {
        tracing::debug!(?req, "Invalid or missing authorization.");
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
