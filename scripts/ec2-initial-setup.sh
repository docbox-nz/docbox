#!/bin/bash

# ===
# This setup script is intended to be run on the EC2 instance
# that will be running the API.
# ===

# Variables injected into the template
PROXY_HOST="${proxy_host}"
PROXY_PORT="${proxy_port}"
PROXY_URL="http://$PROXY_HOST:$PROXY_PORT"

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

# Set the system timezone to NZST
set_timezone() {
    sudo timedatectl set-timezone Pacific/Auckland
}

# Setup the package sources
# (We required Bookworm backports to get the latest LibreOffice versions)
configure_sources() {
    echo 'deb http://deb.debian.org/debian/ bookworm main' | sudo tee /etc/apt/sources.list.d/debian.list

    # Bookworm backports for latest available LibreOffice version
    echo 'deb http://deb.debian.org/debian bookworm-backports main' | sudo tee /etc/apt/sources.list.d/bookworm-backports.list
}

# Install required dependencies
install_dependencies() {
    # Install updates
    echo "Installing updates"
    sudo apt-get update

    # Install poppler-utils, and cron
    echo "Installing dependencies"
    sudo apt-get install -y poppler-utils cron

    # Install libreoffice from backports
    echo "Installing libreoffice"
    sudo apt install -y -t stable-backports libreoffice
}

# Setup the office conversion server
setup_converter_server() {
    local TMP_SERVER_PATH="/tmp/office-convert-server"
    local SERVER_PATH="/docbox/office-convert-server"

    # Download office converter server binary
    echo "Downloading converter server"
    curl -L -o $TMP_SERVER_PATH https://github.com/jacobtread/office-convert-server/releases/download/v0.3.0/office-convert-server-arm64

    # Ensure docbox directory exists
    sudo mkdir /docbox

    # Move office convert server binary
    sudo mv $TMP_SERVER_PATH $SERVER_PATH

    # Ensure the binary has execute permissions
    sudo chmod +x $SERVER_PATH

    # Create convert server service (Libreoffice conversion)
    echo "Creating convert server service"
    cat <<EOF | sudo tee /etc/systemd/system/convert-server.service >/dev/null
[Unit]
Description=Convert Server Service
After=network.target

[Service]
Type=simple
ExecStart=$${SERVER_PATH} --host 0.0.0.0 --port 8081
Restart=on-failure

[Install]
WantedBy=multi-user.target
EOF

    echo "Reloading systemd manager configuration..."

    # Reload the services
    sudo systemctl daemon-reload

    # Enable automatic startup of the service
    sudo systemctl enable convert-server.service

    # Start the services
    sudo systemctl start convert-server.service
}

# Start and enable the cron service
setup_cron() {
    sudo systemctl start cron
    sudo systemctl enable cron
}

# Setup a background cron job to restart the conversion server
# at 1 AM each day to free any memory that couldn't be properly collected
# through garbage collection
setup_convert_server_restart_job() {
    # Add a cron job to restart convert-server.service at 1 AM NZST
    # this is being added to the root users crontab
    local CRON_JOB="0 1 * * * /usr/bin/systemctl restart convert-server.service"

    # Check if the cron job already exists to avoid duplication
    (sudo crontab -l 2>/dev/null | grep -q "$CRON_JOB") || (
        echo "Setting up cron job..."
        (
            sudo crontab -l 2>/dev/null
            echo "$CRON_JOB"
        ) | sudo crontab -
    )
}

# Setup a background cron job to automatically
# run the garbage collection on the office conversion
# server to keep memory usage low
setup_convert_server_garbage_job() {
    # Hourly cron job to run the garbage collection on libreoffice through the convert server
    local CRON_JOB_COLLECT_GARBAGE="0 * * * * /usr/bin/curl -X POST http://localhost:8081/collect-garbage"

    # Check if the cron job already exists to avoid duplication
    (sudo crontab -l 2>/dev/null | grep -q "$CRON_JOB_COLLECT_GARBAGE") || (
        echo "Setting up garbage cron job..."
        (
            sudo crontab -l 2>/dev/null
            echo "$CRON_JOB_COLLECT_GARBAGE"
        ) | sudo crontab -
    )
}

# Create the docbox directory, download docbox binary and setup the docbox service
setup_docbox_service() {
    local TMP_SERVER_PATH="/tmp/docbox"
    local SERVER_PATH="/docbox/app"

    # Download office converter server binary
    echo "Downloading converter server"
    curl -L -o $TMP_SERVER_PATH https://github.com/docbox-nz/docbox/releases/download/v0.1.0/docbox-aarch64-linux-gnu

    # Ensure the docbox directory exists
    sudo mkdir /docbox

    # Move docbox server binary
    sudo mv $TMP_SERVER_PATH $SERVER_PATH

    # Ensure the binary has execute permissions
    sudo chmod +x $SERVER_PATH

    # Create service for docbox
    echo "Creating docbox service"
    cat <<EOF | sudo tee /etc/systemd/system/docbox.service >/dev/null
[Unit]
Description=DocBox service
After=network-online.target

[Service]
Type=simple
ExecStart=/docbox/app
Restart=always
WorkingDirectory=/docbox
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
EOF

    echo "Reloading systemd manager configuration..."

    # Reload the services
    sudo systemctl daemon-reload

    # Enable automatic startup of the services
    sudo systemctl enable docbox.service

    # Start the services
    sudo systemctl start docbox.service
}

configure_proxy
wait_for_network
set_timezone
configure_sources
install_dependencies
setup_converter_server
setup_cron
setup_convert_server_restart_job
setup_convert_server_garbage_job
setup_docbox_service
