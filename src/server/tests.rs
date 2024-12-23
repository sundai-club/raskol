use std::time::Duration;

use axum::{
    body::{Body, to_bytes},
    extract::State,
    http::{self, Request, StatusCode},
    Router,
};
use serde_json::json;
use tower::util::ServiceExt;

use crate::jwt;

const MAX_BODY_SIZE: usize = 1024 * 1024; // 1MB limit

#[tokio::test]
async fn test_chat_endpoint() {
    let groq_api_key = std::env::var("GROQ_API_KEY")
        .expect("GROQ_API_KEY must be set to run this test");

    // Create JWT options and token
    let jwt_opts = jwt::Options {
        secret: "test-secret".to_string(),
        audience: "test".to_string(),
        issuer: "test".to_string(),
    };
    let claims = crate::auth::Claims::new("test-user", Duration::from_secs(300))
        .expect("Failed to create claims");
    let token = claims.to_str(&jwt_opts).expect("Failed to create token");

    // Create test app
    let hits = super::Hits::new();
    let groq_config = super::GroqConfig::new(groq_api_key);
    
    let app = Router::new()
        .route(
            "/api/chat",
            axum::routing::post(super::handle_chat),
        )
        .route_layer(axum::middleware::from_fn({
            let jwt_opts = jwt_opts.clone();
            move |req, next| super::auth_layer(jwt_opts.clone(), req, next)
        }))
        .with_state((hits, groq_config));

    // Create test request
    let payload = json!({
        "messages": [{
            "role": "user",
            "content": "Say 'test' and nothing else"
        }],
        "model": "mixtral-8x7b-32768",
        "temperature": 0.0,
        "max_tokens": 10
    });

    // Print request payload
    println!("Request payload: {}", serde_json::to_string_pretty(&payload).unwrap());

    let request = Request::builder()
        .method(http::Method::POST)
        .uri("/api/chat")
        .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    // Send request and check response
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), MAX_BODY_SIZE).await.unwrap();
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    tracing::info!(?response, "Got response from Groq");
    
    assert!(response.get("choices").is_some(), "Response should have choices");
}

#[tokio::test]
async fn test_image_analysis_chat() {
    let groq_api_key = std::env::var("GROQ_API_KEY")
        .expect("GROQ_API_KEY must be set to run this test");

    // Create JWT options and token
    let jwt_opts = jwt::Options {
        secret: "test-secret".to_string(),
        audience: "test".to_string(),
        issuer: "test".to_string(),
    };
    let claims = crate::auth::Claims::new("test-user", Duration::from_secs(300))
        .expect("Failed to create claims");
    let token = claims.to_str(&jwt_opts).expect("Failed to create token");

    // Create test app
    let hits = super::Hits::new();
    let groq_config = super::GroqConfig::new(groq_api_key);
    
    let app = Router::new()
        .route(
            "/api/chat",
            axum::routing::post(super::handle_chat),
        )
        .route_layer(axum::middleware::from_fn({
            let jwt_opts = jwt_opts.clone();
            move |req, next| super::auth_layer(jwt_opts.clone(), req, next)
        }))
        .with_state((hits, groq_config));

    // Create test request that mimics your Node.js example
    let image_url = "https://example.com/test-image.jpg";
    let payload = json!({
        "messages": [
            {
                "role": "system",
                "content": "You are an expert at analyzing Instagram photos, particularly skilled at identifying specific sports and activities. \
                When you see someone skiing, say \"skiing\". When you see someone fishing, say \"fishing\". Be extremely specific about activities."
            },
            {
                "role": "user",
                "content": format!(
                    "Analyze this Instagram photo and identify the EXACT activities or sports shown: {}\n\n\
                    Focus on answering these questions with extreme specificity:\n\
                    1. What SPECIFIC activity or sport is being performed? (e.g., \"downhill skiing\" not just \"winter sports\")\n\
                    2. What equipment is being used? (e.g., \"fishing rod\", \"ski poles\")\n\
                    3. What exact environment is this taking place in? (e.g., \"ski slope\", \"fishing boat\")\n\n\
                    Be extremely precise and concrete in identifying activities.",
                    image_url
                )
            }
        ],
        "model": "mixtral-8x7b-32768",
        "temperature": 0.3,
        "max_tokens": 200
    });

    // Print request payload
    println!("Request payload: {}", serde_json::to_string_pretty(&payload).unwrap());

    let request = Request::builder()
        .method(http::Method::POST)
        .uri("/api/chat")
        .header(http::header::AUTHORIZATION, format!("Bearer {}", token))
        .header(http::header::CONTENT_TYPE, "application/json")
        .body(Body::from(serde_json::to_string(&payload).unwrap()))
        .unwrap();

    // Send request and check response
    let response = app.oneshot(request).await.unwrap();
    
    assert_eq!(response.status(), StatusCode::OK);
    
    let body = to_bytes(response.into_body(), MAX_BODY_SIZE).await.unwrap();
    let response_str = String::from_utf8_lossy(&body);
    println!("Raw response: {}", response_str);
    
    let response: serde_json::Value = serde_json::from_slice(&body).unwrap();
    
    tracing::info!(?response, "Got response from Groq");
    
    // Print full response structure
    println!("Full response structure: {}", serde_json::to_string_pretty(&response).unwrap());
    
    // Check if we got an error response
    if let Some(error) = response.get("error") {
        println!("Error from Groq: {:?}", error);
    }
    
    assert!(response.get("choices").is_some(), "Response should have choices");
    
    // Print the actual response content
    if let Some(choices) = response.get("choices") {
        if let Some(first_choice) = choices.as_array().and_then(|arr| arr.first()) {
            if let Some(message) = first_choice.get("message") {
                if let Some(content) = message.get("content") {
                    println!("Analysis result: {}", content);
                }
            }
        }
    }
} 