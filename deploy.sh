#!/bin/bash
set -euo pipefail

# Define variables at the top
volume_name="raskol_data"
region="dfw"

# Install flyctl if not present
if ! command -v flyctl &> /dev/null; then
    curl -L https://fly.io/install.sh | sh
fi

# Login if needed
if ! flyctl auth whoami &> /dev/null; then
    flyctl auth login
fi

# Destroy existing app if it exists
echo "Cleaning up existing deployment..."
if flyctl status &> /dev/null; then
    echo "Destroying existing app..."
    flyctl volumes destroy "$volume_name" --yes || true
    flyctl apps destroy --yes || true
fi

# Create new app
echo "Creating new app..."
flyctl launch --no-deploy

# Create volume if it doesn't exist
echo "Ensuring volume exists..."
echo "Creating volume $volume_name in region $region..."
flyctl volumes create "$volume_name" -r "$region" -n 1 || {
    echo "Failed to create volume $volume_name in region $region"
    exit 1
}

# Set secrets with verification
set_secret() {
    local name=$1
    local value=$2
    echo "Setting $name..."
    
    output=$(flyctl secrets set "$name=$value" 2>&1)
    status=$?
    echo "$output"
    
    if [ $status -eq 0 ] || \
       [[ $output =~ "Secrets are staged for the next deployment" ]] || \
       [[ $output =~ "Machine".*"update succeeded" ]]; then
        echo "Successfully set $name"
        return 0
    else
        echo "Failed to set $name (exit code: $status)"
        return 1
    fi
}

# Set all secrets
echo "Setting secrets..."
set_secret "JWT_SECRET" "super-secret" || exit 1
set_secret "JWT_AUDIENCE" "authenticated" || exit 1
set_secret "JWT_ISSUER" "https://bright-kitten-41.clerk.accounts.dev" || exit 1
set_secret "TARGET_AUTH_TOKEN" "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" || exit 1
set_secret "RUST_LOG" "debug,raskol=debug" || exit 1

# Deploy application
echo "Deploying application..."
flyctl deploy --verbose

# Show logs after deployment
echo "Showing recent logs..."
flyctl logs

# Wait a moment and check if the app is healthy
sleep 5
echo "Checking app health..."
flyctl status

# Debug: Check the data directory
echo "Checking data directory..."
flyctl ssh console -C "ls -la /data" 