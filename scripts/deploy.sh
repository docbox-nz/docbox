# Extract EC2 instance IP from the terraform output
EC2_HOST=admin@$(terraform -chdir=./terraform output -raw api_private_ip)

# Building binary
BINARY_PATH="./target/aarch64-unknown-linux-musl/release/docbox"
REMOTE_BINARY_PATH="/docbox/app"
REMOTE_BINARY_TMP_PATH="/tmp/docbox"
SERVICE_NAME="docbox"

LOCAL_ENV_PATH=".env.production"
REMOTE_ENV_TMP_PATH="/tmp/docbox.env"
REMOTE_ENV_PATH="/docbox/.env"

# Default values for variables
BINARY=false
ENV=false
SETUP=false

# Parse arguments
while [[ $# -gt 0 ]]; do
    case $1 in
    --binary)
        BINARY=true
        shift
        ;;
    --env)
        ENV=true
        shift
        ;;
    *)
        echo "Unknown argument: $1"
        echo "Usage: $0 [--binary|--env]"
        exit 1
        ;;
    esac
done

if [ "$ENV" = true ]; then
    # Copy the env variables to the remote server
    echo "Copying env to the remote server..."
    scp -A $LOCAL_ENV_PATH $EC2_HOST:$REMOTE_ENV_TMP_PATH

    # Move the env to expected path
    ssh -A $EC2_HOST "sudo mv $REMOTE_ENV_TMP_PATH $REMOTE_ENV_PATH"
fi

if [ "$BINARY" = true ]; then
    # Copy the binary to the remote server
    echo "Copying binary to the remote server..."
    scp -A $BINARY_PATH $EC2_HOST:$REMOTE_BINARY_TMP_PATH

    # Move the binary to expected path
    ssh -A $EC2_HOST "sudo mv $REMOTE_BINARY_TMP_PATH $REMOTE_BINARY_PATH"

    # Mark executable file as executable
    ssh -A $EC2_HOST "sudo chmod +x $REMOTE_BINARY_PATH"

    # Restart docbox service
    ssh -A $EC2_HOST "sudo systemctl restart $SERVICE_NAME"
fi
