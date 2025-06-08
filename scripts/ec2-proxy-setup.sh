#!/bin/bash

# Install Squid
sudo yum install -y squid

# Configure Squid
sudo bash -c 'cat << EOF > /etc/squid/squid.conf
# Set squid server port
http_port 3128

# Anonymize client IP address
forwarded_for delete

# Allow access from all (Security access is done on the AWS side)
http_access allow all
EOF'

# Restart Squid service
sudo systemctl restart squid

# Enable Squid service on start
sudo systemctl enable squid
