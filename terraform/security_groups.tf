# Security group for Opensearch
#
# Allows access from the VPN and from the 
# docbox API EC2 instance
resource "aws_security_group" "opensearch_sg" {
  name        = "opensearch_sg"
  description = "Security group for Opensearch, allows access from the docbox EC2 and the VPN"
  vpc_id      = var.vpc_id

  # Allow incoming opensearch traffic from the EC2 instance
  ingress {
    from_port       = 443
    to_port         = 443
    protocol        = "tcp"
    security_groups = [aws_security_group.docbox_sg.id] # EC2 Security Group
  }

  # Allow access through VPN
  ingress {
    from_port       = 0
    to_port         = 0
    protocol        = "-1"
    security_groups = [var.vpn_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "docbox-opensearch-sg"
  }
}

# Security group for the docbox EC2 instance
#
# Allows access from the VPN and to the API 
resource "aws_security_group" "docbox_sg" {
  name        = "docbox-sg"
  description = "Security group for the docbox EC2, allows access from VPN and Provida API"
  vpc_id      = var.vpc_id

  # Allow access through VPN
  ingress {
    from_port       = 0
    to_port         = 0
    protocol        = "-1"
    security_groups = [var.vpn_security_group_id]
  }

  # Allow access to the API
  ingress {
    from_port   = 8080
    to_port     = 8080
    protocol    = "tcp"
    cidr_blocks = [var.vpc_cidr]
  }

  # Allow ingres from 443 on the private subnet, used by AWS Secrets manager
  # requests to the secrets manager will timeout without this 
  ingress {
    from_port   = 443
    to_port     = 443
    protocol    = "tcp"
    cidr_blocks = ["0.0.0.0/0"]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "docbox-docbox-sg"
  }
}

# Security group for the HTTP proxy
#
# Allows access from all the private subnet to make HTTP requests
# to the public internet
resource "aws_security_group" "http_proxy_sg" {
  name        = "docbox-http-proxy-sg"
  description = "Docbox HTTP proxy security gruup, allows HTTP access for all resources on the private subnet & allows VPN access"
  vpc_id      = var.vpc_id

  ingress {
    from_port = 3128
    to_port   = 3128
    protocol  = "tcp"
    # Allow all the private subnets to access the proxy
    cidr_blocks = [
      aws_subnet.private_subnet.cidr_block,
    ]
  }

  # Allow access through VPN
  ingress {
    from_port       = 0
    to_port         = 0
    protocol        = "-1"
    security_groups = [var.vpn_security_group_id]
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "docbox-http-proxy-sg"
  }
}

