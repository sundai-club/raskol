# Raskol

Share 1 API key with N group members without revealing it. Raskol acts as a proxy server that authenticates users with JWTs and forwards their requests to the target API (currently configured for Groq's API).

## Prerequisites

- Rust toolchain (latest stable)
- For release builds: `musl` target (`rustup target add x86_64-unknown-linux-musl`)
- OpenSSL (for TLS certificate generation if using HTTPS)

## Quick Start

1. Clone the repository
2. Create a configuration file:

   ```bash
   mkdir -p conf
   # The server will auto-generate a default conf.toml on first run
   cargo run -- server
   ```

3. Edit `conf/conf.toml` with your settings:

   ```toml
   log_level = "INFO"  # or DEBUG, WARN, ERROR
   addr = "127.0.0.1"  # IP to bind to
   port = 8080         # Port to listen on

   # JWT configuration for authentication
   [jwt]
   secret = "your-secure-secret-here"
   audience = "authenticated"
   issuer = "your-issuer-identifier"

   # Target API configuration
   target_address = "api.groq.com"  # The API you're proxying to
   target_auth_token = "your-actual-api-key"  # The API key you're protecting

   # Rate limiting settings
   min_hit_interval = 5.0  # Minimum seconds between requests
   max_tokens_per_day = 1000000  # Maximum tokens per user per day
   sqlite_busy_timeout = 60.0

   # Optional TLS configuration
   [tls]
   cert_file = "data/cert/cert.pem"
   key_file = "data/cert/key.pem"
   ```

4. Generate a JWT for a user:

   ```bash
   # Format: cargo run -- jwt <user_id> <ttl_in_seconds>
   cargo run -- jwt "user123" 86400  # Creates a token valid for 24 hours
   ```

   The generated JWT will include:

   - `sub`: The user ID you provided
   - `exp`: Expiration timestamp
   - `role`: Default role "USER"

5. Run the server:
   ```bash
   cargo run -- server
   ```

## Making Requests

To make requests through the proxy:

1. Use the JWT token as a Bearer token in the Authorization header
2. Make POST requests to `http://localhost:8080/<endpoint>`
3. The request will be forwarded to `https://<target_address>/<endpoint>`

Example using curl:

```
curl -i http://localhost:8080/openai/v1/chat/completions \
  -H 'Authorization: Bearer eyJhbGciOiJIUzI1NiIsImNhdCI6ImNsX0I3ZDRQRDIyMkFBQSIsInR5cCI6IkpXVCJ9.eyJhcHBfbWV0YWRhdGEiOnt9LCJhdWQiOiJhdXRoZW50aWNhdGVkIiwiZW1haWwiOiJjZGlya3M0K3Rlc3RAbWUuY29tIiwiZXhwIjoxNzM2ODgwNjUzLCJpYXQiOjE3MzYyODA2NTMsImlzcyI6Imh0dHBzOi8vYnJpZ2h0LWtpdHRlbi00MS5jbGVyay5hY2NvdW50cy5kZXYiLCJqdGkiOiIyNWQ3YTEwNDc2ZTUzNTkyM2E3OSIsIm5iZiI6MTczNjI4MDY0OCwicm9sZSI6IkhBQ0tFUiIsInN1YiI6InVzZXJfMnJKYTNxaW50TXU0Vm1lVEdIVlhXNVdqWGEzIiwidXNlcl9tZXRhZGF0YSI6e319.09TVUm3zFRYqjHJhtlknknVhOPtclDwUlK6X6MHTSwg' \
  -H 'Content-Type: application/json' \
  -X POST \
  --json '{"messages": [{"role": "user", "content": "write a banger tweet for c programmers"}], "model": "llama3-8b-8192"}'
```

## Authorization

The API uses role-based access control with JWT tokens. There are two privileged roles:

- `HACKER`: Default role for API access
- `ADMIN`: Administrative role with the same permissions as HACKER

Both roles can:

- Make API requests through the proxy
- View usage statistics
- Access token usage information

Users without these roles will receive a 403 Forbidden response when attempting to access protected endpoints.

## API Documentation

The API documentation is available through a Swagger UI interface at:

`http://localhost:8080/swagger-ui`

This interactive documentation allows you to:

- Browse all available endpoints
- See request/response schemas
- Test endpoints directly from the browser
- View authentication requirements
- Understand rate limiting and token usage

## API Endpoints

- `POST /{endpoint}`: Forwards requests to the target API (requires HACKER or ADMIN role)
- `GET /ping`: Health check endpoint (requires HACKER or ADMIN role)
- `GET /stats`: Returns usage statistics for the authenticated user (requires HACKER or ADMIN role)
- `GET /total-stats`: Returns usage statistics for all users (requires ADMIN role)

### Stats Endpoints

The stats endpoints provide usage statistics:

1. `/stats` - Returns the authenticated user's own stats (HACKER or ADMIN role)
2. `/total-stats` - Returns stats for all users in the system (ADMIN only)

Stats response format:

```

```

## Deploying to fly.io

1. Install the flyctl CLI:

   ```bash
   curl -L https://fly.io/install.sh | sh
   ```

2. Login to fly.io:

   ```bash
   fly auth login
   ```

3. Create a new fly.io app:

   ```bash
   fly launch
   ```

4. Create a `Dockerfile`:

   ```dockerfile
   FROM rust:1.75-slim-bookworm as builder
   WORKDIR /usr/src/raskol
   COPY . .
   RUN cargo build --release

   FROM debian:bookworm-slim
   COPY --from=builder /usr/src/raskol/target/release/raskol /usr/local/bin/
   COPY --from=builder /usr/src/raskol/conf/conf.toml /etc/raskol/
   CMD ["raskol", "server"]
   ```

5. Create a `fly.toml`:

   ```toml
   app = "your-app-name"
   primary_region = "dfw"

   [build]
   dockerfile = "Dockerfile"

   [http_service]
   internal_port = 8080
   force_https = true
   auto_stop_machines = true
   auto_start_machines = true
   min_machines_running = 1
   processes = ["app"]

   [[http_service.ports]]
   port = 80
   handlers = ["http"]
   force_https = true

   [[http_service.ports]]
   port = 443
   handlers = ["tls", "http"]
   ```

6. Set your configuration secrets:

   ```bash
   fly secrets set JWT_SECRET="your-secure-secret"
   fly secrets set TARGET_AUTH_TOKEN="your-api-key"
   ```

7. Deploy:
   ```bash
   fly deploy
   ```

Your API will be available at `https://your-app-name.fly.dev`.

Note: Make sure to update your `conf.toml` to use environment variables for sensitive data:

```toml
[jwt]
secret = "${JWT_SECRET}"

target_auth_token = "${TARGET_AUTH_TOKEN}"
```
