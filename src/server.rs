use std::{env, net::SocketAddr, path::PathBuf};

use anyhow::{anyhow, Context};
use axum::{
    extract::{ConnectInfo, Path, Request, State},
    http::{header, StatusCode},
    middleware::{self, Next},
    response::{Response, Result},
    routing::get,
    Json,
    body::{Body, Bytes},
};

use crate::{
    auth,
    chat,
    conf,
    data::Storage,
    types::UserStats,
    data::UsageStats,
};

use utoipa::OpenApi;
use utoipa_swagger_ui::SwaggerUi;
use crate::docs::ApiDoc;
use tower_http::cors::CorsLayer;
use axum::http::Method;
use reqwest::Client;

const MAX_BODY_SIZE: usize = 1024 * 1024 * 10; // 10MB limit

#[derive(Debug)]
pub struct ApiError(StatusCode, Json<ErrorResponse>);

impl From<StatusCode> for ApiError {
    fn from(status: StatusCode) -> Self {
        ApiError(status, Json(ErrorResponse {
            error: status.canonical_reason().unwrap_or("Unknown error").to_string(),
            details: None,
        }))
    }
}

impl From<(StatusCode, Json<ErrorResponse>)> for ApiError {
    fn from((status, error): (StatusCode, Json<ErrorResponse>)) -> Self {
        ApiError(status, error)
    }
}

impl axum::response::IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let ApiError(status, body) = self;
        (status, body).into_response()
    }
}

#[tracing::instrument(name = "server", skip_all)]
pub async fn run() -> anyhow::Result<()> {
    let conf = conf::global();
    let dir = env::current_dir()?;
    
    // Ensure data_dir is relative to current directory if not absolute
    let data_dir = if conf.data_dir.starts_with('/') {
        PathBuf::from(&conf.data_dir)
    } else {
        dir.join(&conf.data_dir)
    };
    
    let db_path = data_dir.join("data.db");
    tracing::info!(?dir, ?conf, data_dir = ?data_dir, db_path = ?db_path, "Starting.");
    let addr = SocketAddr::from((conf.addr, conf.port));
    
    // Create data directory if it doesn't exist
    tokio::fs::create_dir_all(&data_dir).await
        .context("Failed to create data directory")?;
    
    let storage = Storage::connect().await?;
    let state = AppState { storage };
    
    let cors = CorsLayer::new()
        .allow_origin([
            "http://localhost:3000".parse().unwrap(),
            "https://localhost:3000".parse().unwrap(),
            format!("http://{}:{}", conf.addr, conf.port).parse().unwrap(),
            format!("https://{}:{}", conf.addr, conf.port).parse().unwrap(),
        ])
        .allow_methods([Method::GET, Method::POST, Method::OPTIONS])
        .allow_headers([
            header::AUTHORIZATION,
            header::CONTENT_TYPE,
            header::ACCEPT,
            header::ORIGIN,
        ])
        .allow_credentials(true)
        .expose_headers([
            header::CONTENT_TYPE,
            header::CONTENT_LENGTH,
        ]);
    let routes = axum::Router::new()
        .route("/health", get(health_check))
        .merge(SwaggerUi::new("/swagger-ui").url("/api-docs/openapi.json", ApiDoc::openapi()))
        .layer(cors)
        .nest(
            "/",
            axum::Router::new()
                .route("/ping", get(handle_ping))
                .route("/stats", get(stats_handler))
                .route("/total-stats", get(total_stats_handler))
                .route(
                    "/*endpoint",
                    axum::routing::post(handle_api),
                )
                .route_layer(middleware::from_fn(auth_layer)),
        )
        .route_layer(middleware::from_fn({
            |req, next: Next| REQ_ID.scope(ReqId::new(), next.run(req))
        }))
        .with_state(state);

    let service = routes.into_make_service_with_connect_info::<SocketAddr>();

    match &conf.tls {
        None => {
            let listener = tokio::net::TcpListener::bind(addr).await?;
            tracing::warn!(?addr, "Listening unencrypted.");
            axum::serve(listener, service).await?;
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
            axum_server::bind_rustls(addr, config)
                .serve(service)
                .await?;
        }
    }

    Ok(())
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid,
        role = USER.get().role
    )
)]
#[utoipa::path(
    get,
    path = "/ping",
    responses(
        (status = 200, description = "Ping successful", body = String),
        (status = 403, description = "Unauthorized role"),
    ),
    security(
        ("jwt" = [])
    )
)]
async fn handle_ping(
    ConnectInfo(from): ConnectInfo<SocketAddr>,
) -> Result<&'static str, ApiError> {
    let user: User = USER.get();
    
    if user.role != "HACKER" && user.role != "ADMIN" {
        tracing::warn!(user_role = ?user.role, "Unauthorized role attempted to ping.");
        return Err(StatusCode::FORBIDDEN.into());
    }

    tracing::info!(?from, "Handling ping request.");
    Ok("pong")
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid,
        role = USER.get().role
    )
)]
#[utoipa::path(
    post,
    path = "/{endpoint}",
    params(
        ("endpoint" = String, Path, description = "The LLM endpoint path (e.g., 'openai/v1/chat/completions')")
    ),
    request_body = Req,
    responses(
        (status = 200, description = "Chat completion successful", content_type = "application/json"),
        (status = 401, description = "Missing or invalid JWT token"),
        (status = 403, description = "Unauthorized role"),
        (status = 429, description = "Rate limit exceeded or token budget exceeded"),
        (status = 500, description = "Internal server error"),
        (status = 503, description = "Service unavailable - External LLM service error")
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn handle_api(
    State(state): State<AppState>,
    ConnectInfo(_addr): ConnectInfo<SocketAddr>,
    Path(endpoint): Path<String>,
    request: Request<Body>,
) -> Result<Response<Body>> {
    let conf = conf::global();
    let (_parts, body) = request.into_parts();
    let bytes = axum::body::to_bytes(body, MAX_BODY_SIZE).await.map_err(|e| {
        tracing::error!(error = ?e, "Failed to read request body");
        status_to_error(StatusCode::BAD_REQUEST)
    })?;

    // Parse the request body
    let chat_request: chat::Req = serde_json::from_slice(&bytes).map_err(|e| {
        tracing::error!(error = ?e, "Failed to parse request body");
        status_to_error(StatusCode::BAD_REQUEST)
    })?;

    // Get user info for tracking
    let user = USER.get();
    
    // Check token budget
    let tokens_estimate = chat_request.tokens_estimate();
    if !state.storage.tokens_check(&user.uid, tokens_estimate).await? {
        return Err(status_to_error(StatusCode::TOO_MANY_REQUESTS));
    }

    // Create reqwest client
    let client = Client::new();
    
    // Construct target URL
    let target_url = format!("https://{}/{}", conf.target_address, endpoint);
    
    tracing::debug!(?target_url, "Forwarding request to target service");

    // Forward the request
    let response = client
        .post(&target_url)
        .header("Authorization", format!("Bearer {}", conf.target_auth_token))
        .header("Content-Type", "application/json")
        .body(bytes)
        .send()
        .await
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to forward request");
            status_to_error(StatusCode::BAD_GATEWAY)
        })?;

    // Get status and body
    let status = response.status();
    let body_bytes = response.bytes().await.map_err(|e| {
        tracing::error!(error = ?e, "Failed to read response body");
        status_to_error(StatusCode::INTERNAL_SERVER_ERROR)
    })?;

    // If request was successful, consume tokens
    if status.is_success() {
        if let Err(e) = state.storage.tokens_consume(&user.uid, tokens_estimate).await {
            tracing::error!(error = ?e, "Failed to consume tokens");
        }
    }

    // Convert response back to axum response
    Ok(Response::builder()
        .status(status)
        .header("Content-Type", "application/json")
        .body(Body::from(body_bytes))
        .map_err(|e| {
            tracing::error!(error = ?e, "Failed to create response");
            status_to_error(StatusCode::INTERNAL_SERVER_ERROR)
        })?)
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid,
        role = USER.get().role
    )
)]
#[utoipa::path(
    get,
    path = "/stats",
    responses(
        (status = 200, description = "User stats retrieved successfully", body = UserStats),
        (status = 403, description = "Unauthorized role"),
        (status = 500, description = "Internal server error"),
    ),
    security(
        ("jwt" = [])
    )
)]
pub async fn stats_handler(
    State(state): State<AppState>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
) -> Result<Json<UserStats>, StatusCode> {
    let user: User = USER.get();
    
    // Allow both HACKER and ADMIN roles
    if user.role != "HACKER" && user.role != "ADMIN" {
        tracing::warn!(user_role = ?user.role, "Unauthorized role attempted to view stats.");
        return Err(StatusCode::FORBIDDEN);
    }

    tracing::info!(?from, "Handling stats request for user {}.", user.uid);
    
    let stats = state.storage.get_user_stats(&user.uid)
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to get user stats.");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(stats))
}

#[tracing::instrument(
    skip_all,
    fields(
        req_id = REQ_ID.get().req_id,
        uid = USER.get().uid,
        role = USER.get().role
    )
)]
#[utoipa::path(
    get,
    path = "/total-stats",
    responses(
        (status = 200, description = "All user stats retrieved successfully", body = Vec<UserStats>),
        (status = 403, description = "Not an admin"),
        (status = 500, description = "Internal server error"),
    ),
    security(
        ("jwt" = [])
    )
)]
async fn total_stats_handler(
    State(state): State<AppState>,
    ConnectInfo(from): ConnectInfo<SocketAddr>,
) -> Result<Json<Vec<UserStats>>, StatusCode> {
    let user: User = USER.get();
    
    // Only allow ADMIN role
    if user.role != "ADMIN" {
        tracing::warn!(user_role = ?user.role, "Non-admin user attempted to view total stats.");
        return Err(StatusCode::FORBIDDEN);
    }

    tracing::info!(?from, "Handling total stats request from admin {}.", user.uid);
    
    let stats = state.storage.get_all_user_stats()
        .await
        .map_err(|error| {
            tracing::error!(?error, "Failed to get all user stats.");
            StatusCode::INTERNAL_SERVER_ERROR
        })?;

    Ok(Json(stats))
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Debug, Clone)]
struct User {
    pub uid: String,
    pub role: String,
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
) -> Result<Response, ApiError> {
    let conf = conf::global();
    
    // Log the incoming request headers
    tracing::debug!(headers = ?req.headers(), "Incoming request headers");
    
    let auth_token = req
        .headers()
        .get(header::AUTHORIZATION)
        .ok_or_else(|| {
            tracing::warn!("Missing Authorization header");
            StatusCode::UNAUTHORIZED
        })?
        .to_str()
        .map_err(|e| {
            tracing::warn!(error = ?e, "Invalid Authorization header encoding");
            StatusCode::UNAUTHORIZED
        })?
        .strip_prefix("Bearer ")
        .ok_or_else(|| {
            tracing::warn!("Authorization header missing 'Bearer ' prefix");
            StatusCode::UNAUTHORIZED
        })?;

    tracing::debug!(token_length = auth_token.len(), "Received auth token");

    match authorize(auth_token, &conf.jwt) {
        Some(user) => {
            tracing::info!(
                user_id = ?user.uid,
                user_role = ?user.role,
                "User authenticated successfully"
            );
            Ok(USER.scope(user, next.run(req)).await)
        }
        None => {
            tracing::warn!(
                token_prefix = ?auth_token.get(..10).unwrap_or(""),
                "Authentication failed"
            );
            Err(StatusCode::UNAUTHORIZED.into())
        }
    }
}

fn authorize(auth_token: &str, jwt_conf: &conf::Jwt) -> Option<User> {
    tracing::debug!(
        audience = ?jwt_conf.audience,
        issuer = ?jwt_conf.issuer,
        "Attempting to validate JWT"
    );
    
    auth::Claims::from_str(auth_token, jwt_conf)
        .inspect_err(|error| {
            tracing::warn!(
                ?error,
                token_prefix = ?auth_token.get(..10).unwrap_or(""),
                "JWT validation failed"
            );
        })
        .ok()
        .map(|claims| {
            tracing::debug!(
                subject = ?claims.sub,
                role = ?claims.role,
                "JWT claims parsed successfully"
            );
            User { 
                uid: claims.sub,
                role: claims.role,
            }
        })
}

#[derive(Debug, serde::Serialize)]
struct ErrorResponse {
    error: String,
    details: Option<String>,
}

fn status_to_error(status: StatusCode) -> ApiError {
    status.into()
}

#[derive(Clone)]
pub struct AppState {
    pub storage: Storage,
}

