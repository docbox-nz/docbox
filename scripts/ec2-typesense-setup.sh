# Variables injected into the template
PROXY_HOST="${proxy_host}"
PROXY_PORT="${proxy_port}"
PROXY_URL="http://$PROXY_HOST:$PROXY_PORT"
TYPESENSE_API_KEY="${typesense_api_key}"

# Configure APT to use proxy
# (Will be required to reach the internet)
configure_proxy() {
    echo "Acquire::http::Proxy \"$PROXY_URL\";" >/etc/apt/apt.conf.d/95proxy
    echo "Acquire::https::Proxy \"$PROXY_URL\";" >>/etc/apt/apt.conf.d/95proxy

    # Set environment proxy variables for the session
    export http_proxy="$PROXY_URL"
    export https_proxy="$PROXY_URL"
    export HTTP_PROXY="$PROXY_URL"
    export HTTPS_PROXY="$PROXY_URL"

    # Make proxy settings persistent for all users
    cat >>/etc/environment <<EOF
http_proxy=$PROXY_URL
https_proxy=$PROXY_URL
HTTP_PROXY=$PROXY_URL
HTTPS_PROXY=$PROXY_URL
EOF
}

# Wait until the EC2 container has networking
# (When terraform is setting up this may not happen immediately)
wait_for_network() {
    # Wait until the network is up
    until curl -sSf https://www.google.com >/dev/null; do
        echo "Waiting for network..."
        sleep 5
    done
}

# Install required dependencies
install_typesense() {
    curl -O https://dl.typesense.org/releases/28.0/typesense-server-28.0-arm64.deb
    sudo apt install ./typesense-server-28.0-arm64.deb
    sudo systemctl start typesense-server.service
}

update_typesense_config() {
    # Write the new config
    sudo tee /etc/typesense/typesense-server.ini >/dev/null <<EOF
; Typesense Configuration

[server]

api-address = 0.0.0.0
api-port = 8108
data-dir = /var/lib/typesense
api-key = $${TYPESENSE_API_KEY}
log-dir = /var/log/typesense
EOF

    # Restart the service
    sudo systemctl restart typesense-server.service
}

configure_proxy
wait_for_network
install_typesense
update_typesense_config
