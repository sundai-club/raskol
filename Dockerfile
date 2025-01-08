FROM rust:slim-bookworm as builder
WORKDIR /usr/src/raskol
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates curl gettext-base && rm -rf /var/lib/apt/lists/*
WORKDIR /app

# Create required directories with proper permissions
RUN mkdir -p /etc/raskol /data && \
    chown -R nobody:nogroup /etc/raskol /data

COPY --from=builder /usr/src/raskol/target/release/raskol /usr/local/bin/
COPY conf/conf.toml /etc/raskol/conf.toml.template

ENV RASKOL_CONF=/etc/raskol/conf.toml
ENV RUST_BACKTRACE=1

# Add a startup script to verify environment
COPY <<'EOF' /usr/local/bin/start.sh
#!/bin/sh
set -ex

# Print environment for debugging (excluding sensitive values)
env | grep -v 'SECRET\|TOKEN' || true

# Verify required environment variables
echo "Verifying environment variables..."
for var in JWT_SECRET JWT_AUDIENCE JWT_ISSUER TARGET_AUTH_TOKEN; do
    if [ -z "$(eval echo \$$var)" ]; then
        echo "Error: $var is not set"
        exit 1
    fi
done

# Process the configuration template
echo "Processing configuration template..."
envsubst '$JWT_SECRET $JWT_AUDIENCE $JWT_ISSUER $TARGET_AUTH_TOKEN' < /etc/raskol/conf.toml.template > "$RASKOL_CONF"

# Check if config file exists and show contents
if [ ! -f "$RASKOL_CONF" ]; then
    echo "Error: Config file not found at $RASKOL_CONF"
    ls -la /etc/raskol/
    exit 1
fi

# Check data directory permissions
if [ ! -w "/data" ]; then
    echo "Error: /data directory is not writable"
    ls -la /
    exit 1
fi

# Print config file contents (excluding sensitive values)
echo "Config file contents:"
grep -v 'secret\|token' "$RASKOL_CONF" || true

echo "Starting raskol server..."
exec raskol server
EOF

RUN chmod +x /usr/local/bin/start.sh && \
    chown nobody:nogroup /usr/local/bin/start.sh

USER nobody
CMD ["/usr/local/bin/start.sh"] 