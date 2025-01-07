use utoipa::OpenApi;
use crate::types::UserStats;
use crate::chat::{Req, Msg};

#[derive(OpenApi)]
#[openapi(
    paths(
        crate::server::handle_ping,
        crate::server::stats_handler,
        crate::server::total_stats_handler,
        crate::server::handle_api,
    ),
    components(
        schemas(UserStats, Req, Msg)
    ),
    modifiers(&SecurityAddon),
    tags(
        (name = "raskol", description = "Raskol API endpoints")
    ),
    info(
        title = "Raskol API",
        version = "1.0",
        description = "API for managing LLM access and usage statistics"
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
                        .scheme(utoipa::openapi::security::HttpAuthScheme::Bearer)
                        .bearer_format("JWT")
                        .description(Some("Enter your JWT token here (without Bearer prefix)"))
                        .build()
                )
            );
            
            // Add global security requirement with proper type conversions
            let security_req = utoipa::openapi::SecurityRequirement::new::<String, Vec<String>, String>(
                "jwt".to_string(), 
                Vec::new()
            );
            openapi.security = Some(vec![security_req]);
        }
    }
} 