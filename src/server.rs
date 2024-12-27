use std::{net::SocketAddr, sync::Arc, time::Duration};

use axum::{
    extract::{Path, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    Json,
};

use crate::{
    auth,
    conf::{self, Conf, ConfJwt},
    data::Storage,
};

#[tracing::instrument(name = "server", skip_all)]
pub async fn run() -> anyhow::Result<()> {
    let conf = conf::global();
    tracing::info!(?conf, "Starting.");
    let addr = SocketAddr::from((conf.addr, conf.port));
    let storage = Storage::connect().await?;
    let routes = axum::Router::new()
        .route(
            "/*endpoint",
            axum::routing::post({
                let storage = storage.clone();
                move |endpoint, payload| handle(storage, endpoint, payload)
            }),
        )
        .route_layer(middleware::from_fn({
            |req, next: Next| REQ_ID.scope(ReqId::new(), next.run(req))
        }))
        .route_layer(middleware::from_fn({
            |req, next: Next| auth_layer(req, next)
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
    storage: Storage,
    Path(endpoint): Path<String>,
    payload: Json<serde_json::Value>,
) -> Result<Response<String>, StatusCode> {
    tracing::debug!("Handling.");
    let conf = conf::global();
    let user: User = USER.get();
    let (hit_count, elapsed_since_prev) =
        storage.hit(&user.uid).await.map_err(|error| {
            tracing::error!(?error, "Failed to hit storage.");
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    let min_hit_interval = Duration::from_secs_f32(conf.min_hit_interval);
    tracing::debug!(
        hit_count,
        ?elapsed_since_prev,
        ?min_hit_interval,
        "Checking interval."
    );
    if elapsed_since_prev < min_hit_interval {
        tracing::warn!("Rejecting. Too close to previous request.");
        return Err(StatusCode::TOO_MANY_REQUESTS);
    };
    let address = &conf.target_address;
    let url = format!("https://{address}/{endpoint}");
    let resp = reqwest::Client::new()
        .post(url)
        .bearer_auth(&conf.target_auth_token)
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
    req: Request,
    next: Next,
) -> Result<Response, StatusCode> {
    let conf: Arc<Conf> = conf::global();
    let auth_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|header| header.to_str().ok())
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if let Some(user) = authorize(auth_token, &conf.jwt).await {
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
