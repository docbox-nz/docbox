#!/bin/bash

# ===
# This setup script is intended to be run on the EC2 instance
# that will be running the API, should be run after the instance
# is setup, based on the terraform config
# ===

mkdir /docbox

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
