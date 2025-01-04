use std::{env, net::SocketAddr, sync::Arc, time::Duration};

use anyhow::{anyhow, Context};
use axum::{
    extract::{ConnectInfo, Path, Request},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    routing::get,
    Json,
};

use crate::{
    auth, chat,
    conf::{self, Conf},
    data::Storage,
};

#[tracing::instrument(name = "server", skip_all)]
pub async fn run() -> anyhow::Result<()> {
    let conf = conf::global();
    let dir = env::current_dir()?;
    tracing::info!(?dir, ?conf, "Starting.");
    let addr = SocketAddr::from((conf.addr, conf.port));
    let storage = Storage::connect().await?;
    let routes = axum::Router::new()
        .route("/ping", get(handle_ping))
        .nest(
            "/",
            axum::Router::new()
                .route(
                    "/*endpoint",
                    axum::routing::post({
                        let storage = storage.clone();
                        move |conn_info, endpoint, payload| {
                            handle_api(storage, conn_info, endpoint, payload)
                        }
                    }),
                )
                .route_layer(middleware::from_fn({
                    |req, next: Next| auth_layer(req, next)
                })),
        )
        .route_layer(middleware::from_fn({
            |req, next: Next| REQ_ID.scope(ReqId::new(), next.run(req))
        }))
        .into_make_service_with_connect_info::<SocketAddr>();

    match &conf.tls {
        None => {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::warn!(?addr, "Listening unencrypted.");
            axum::serve(listener, routes).await?;
        }
        Some(conf::Tls {
            cert_file,
            key_file,
        }) => {
            // XXX One MUST do this manual init of rustls provider when using
            //     more than a single dep which itself depends on rustls.
            //     Here we using 2:
            //     - axum_server
            //     - reqwest
            rustls::crypto::aws_lc_rs::default_provider()
                .install_default()
                .map_err(|crypto_provider| {
                    anyhow!(
                        "Failed to install default crypto provider: \
                        {crypto_provider:?}"
                    )
                })?;

            let config =
                axum_server::tls_rustls::RustlsConfig::from_pem_file(
                    cert_file, key_file,
                )
                .await
                .context(format!(
                    "Failed to construct RustlsConfig. \
                    cert_file={cert_file:?}, key_file={key_file:?}"
                ))?;

            tracing::info!(
                ?addr,
                ?cert_file,
                ?key_file,
                "Listening with TLS."
            );
            axum_server::bind_rustls(addr, config).serve(routes).await?;
        }
    }

    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(req_id = REQ_ID.get().req_id)
)]
async fn handle_ping(
    ConnectInfo(from): ConnectInfo<SocketAddr>,
) -> StatusCode {
    tracing::info!(?from, "Handling ping request.");
    StatusCode::OK
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid
    )
)]
async fn handle_api(
    storage: Storage,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
    Path(endpoint): Path<String>,
    Json(chat_req): Json<chat::Req>,
) -> Result<Response<String>, StatusCode> {
    tracing::info!(?from, "Handling API request.");
    let conf = conf::global();
    let user: User = USER.get();

    //
    // Rate Limit
    //
    let _token_count = chat_req.tokens_estimate();
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
        // TODO Explain reason in response body.
        return Err(StatusCode::TOO_MANY_REQUESTS);
    };

    //
    // Token Budget:
    // 1. check if enough in budget
    // 2. make request
    // 3. consume from budget
    //
    let token_count = chat_req.tokens_estimate();
    let is_enough_tokens_in_budget = storage
        .tokens_check(&user.uid, token_count)
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to hit storage.");
            StatusCode::SERVICE_UNAVAILABLE
        })?;
    if !is_enough_tokens_in_budget {
        tracing::warn!("Rejecting. Token budget exceeded.");
        // TODO Explain reason in response body.
        return Err(StatusCode::TOO_MANY_REQUESTS);
    }

    let address = &conf.target_address;
    let url = format!("https://{address}/{endpoint}");
    let (client, out_req) = reqwest::Client::new()
        .post(url)
        .bearer_auth(&conf.target_auth_token)
        .json(&chat_req)
        .build_split();
    let out_req = out_req.map_err(|error| {
        tracing::error!(?error, "Failed to build outgoing request.");
        StatusCode::INTERNAL_SERVER_ERROR
    })?;
    tracing::debug!(
        out_headers = ?out_req.headers(),
        out_body = ?out_req
            .body()
            .map(|b| b.as_bytes().map(|b| String::from_utf8_lossy(b))),
        "Outgoing reqwest."
    );
    let resp = client.execute(out_req).await.map_err(|error| {
        tracing::error!(?error, "Failed to make the external request.");
        StatusCode::SERVICE_UNAVAILABLE
    })?;

    let status = resp.status();
    let headers = resp.headers().to_owned();
    let code = status.as_u16();
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
    if !status.is_success() {
        tracing::error!(
            ?status,
            ?headers,
            ?body,
            "External request rejected."
        );
        return Err(StatusCode::SERVICE_UNAVAILABLE);
    }
    // XXX If tokens_consume fails - we don't want to fail the request, so
    //     we might end-up not consuming. May need to yell louder here. Alert?
    if let Err(error) = storage.tokens_consume(&user.uid, token_count).await {
        tracing::error!(?error, ?token_count, "Failed to consume tokens!");
    }
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
    if let Some(user) = authorize(auth_token, &conf.jwt) {
        Ok(USER.scope(user, next.run(req)).await)
    } else {
        tracing::debug!(?req, "Invalid or missing authorization.");
        Err(StatusCode::UNAUTHORIZED)
    }
}

fn authorize(auth_token: &str, jwt_conf: &conf::Jwt) -> Option<User> {
    auth::Claims::from_str(auth_token, jwt_conf)
        .inspect_err(|error| tracing::debug!(?error, "Auth failed."))
        .ok()
        .map(|claims| User { uid: claims.sub })
}

fn is_json(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s).is_ok()
}
