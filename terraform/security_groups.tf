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
  description = "Docbox HTTP proxy security group, allows HTTP access for all resources on the private subnet & allows VPN access"
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


# Security group for the typesense server
resource "aws_security_group" "docbox_typesense_sg" {
  name        = "docbox-typesense"
  description = "Security group for typesense"
  vpc_id      = var.vpc_id

  # Allow members of the private subnet access
  ingress {
    from_port = 8108
    to_port   = 8108
    protocol  = "tcp"
    cidr_blocks = [
      aws_subnet.private_subnet.cidr_block,
    ]
    description = "Access from private subnet services"
  }

  # Allow access through VPN
  ingress {
    from_port       = 0
    to_port         = 0
    protocol        = "-1"
    security_groups = [var.vpn_security_group_id]
    description     = "VPN all access"
  }

  egress {
    from_port   = 0
    to_port     = 0
    protocol    = "-1"
    cidr_blocks = ["0.0.0.0/0"]
  }

  tags = {
    Name = "docbox-typesense-sg"
  }
}

