use utoipa::OpenApi;
use utoipa::openapi::security::HttpAuthScheme;
use crate::types::UserStats;
use crate::chat::{Req, Msg, MsgContent, ContentItem, ImageUrl};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::server::handle_ping,
        crate::server::stats_handler,
        crate::server::total_stats_handler,
        crate::server::handle_api,
    ),
    components(
        schemas(
            UserStats, 
            Req, 
            Msg, 
            MsgContent, 
            ContentItem, 
            ImageUrl
        )
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "raskol", description = "Raskol API endpoints")
    ),
    info(
        title = "Raskol API",
        version = "1.0",
        description = "API for managing LLM access and usage statistics",
        contact(
            name = "Connor Dirks & Siraaj Khandkar",
            email = "cdirks4@me.com"
        ),
        license(
            name = "BSD-3-Clause"
        )
    )
)]
pub struct ApiDoc;

struct SecurityAddon;

impl utoipa::Modify for SecurityAddon {
    fn modify(&self, openapi: &mut utoipa::openapi::OpenApi) {
        if let Some(components) = openapi.components.as_mut() {
            components.add_security_scheme(
                "jwt",
                utoipa::openapi::security::SecurityScheme::Http(
                    utoipa::openapi::security::HttpBuilder::new()
                        .scheme(HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .build(),
                ),
            );
        }
    }
} 