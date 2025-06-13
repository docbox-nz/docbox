# SSH public key for SSH access (EC2, PROXY)
resource "aws_key_pair" "ssh_key" {
  key_name   = "docbox_ssh_key"
  public_key = file(var.ssh_public_key_path)

  tags = {
    Name = "docbox-ssh-key"
  }
}

# Docbox API server EC2 
#
# This instance will run:
# - The docbox API HTTP server
# 
# (https://registry.terraform.io/providers/hashicorp/aws/latest/docs/data-sources/instance)
resource "aws_instance" "api" {
  # Debian 12 (20250316-2053) 64-bit (Arm)
  ami           = "ami-01fd140abb2587221"
  instance_type = var.api_instance_type

  subnet_id = aws_subnet.private_subnet.id

  # SSH key access
  key_name = aws_key_pair.ssh_key.key_name

  # Network security group
  vpc_security_group_ids = [aws_security_group.docbox_sg.id]

  iam_instance_profile = aws_iam_instance_profile.docbox_instance_profile.name

  root_block_device {
    volume_type = "gp3"
    volume_size = 8
  }

  # Disable running prolonged higher CPU speeds at a higher cost
  credit_specification {
    cpu_credits = "standard"
  }

  # Pass proxy details into setup script
  user_data = templatefile("../scripts/ec2-docbox-setup.sh", {
    proxy_host = aws_instance.http_proxy.private_ip
    proxy_port = "3128"
  })


  # API must wait for the HTTP proxy to be fully initialized before
  # it can run so that it can use the HTTP proxy to install dependencies
  # (As it does not have regular network access since its in a private subnet)
  depends_on = [aws_instance.http_proxy]

  # Prevent replacement due to user_data changes
  lifecycle {
    ignore_changes = [user_data]
  }

  tags = {
    Name = "docbox-api"
  }
}
# Docbox office conversion server EC2 
#
# This instance will run:
# - Converter HTTP server (Lightweight wrapper for safely interacting with LibreOffice)
# - LibreOffice (Headless for conversion)
# 
# (https://registry.terraform.io/providers/hashicorp/aws/latest/docs/data-sources/instance)
resource "aws_instance" "converter_api" {
  # Debian 12 (20250316-2053) 64-bit (Arm)
  ami           = "ami-01fd140abb2587221"
  instance_type = var.converter_instance_type

  subnet_id = aws_subnet.private_subnet.id

  # SSH key access
  key_name = aws_key_pair.ssh_key.key_name

  # Network security group
  vpc_security_group_ids = [aws_security_group.docbox_converter_sg.id]

  root_block_device {
    volume_type = "gp3"
    volume_size = 8
  }

  # Disable running prolonged higher CPU speeds at a higher cost
  credit_specification {
    cpu_credits = "standard"
  }

  # Pass proxy details into setup script
  user_data = templatefile("../scripts/ec2-converter-setup.sh", {
    proxy_host = aws_instance.http_proxy.private_ip
    proxy_port = "3128"
  })


  # API must wait for the HTTP proxy to be fully initialized before
  # it can run so that it can use the HTTP proxy to install dependencies
  # (As it does not have regular network access since its in a private subnet)
  depends_on = [aws_instance.http_proxy]

  # Prevent replacement due to user_data changes
  lifecycle {
    ignore_changes = [user_data]
  }

  tags = {
    Name = "docbox-converter-api"
  }
}

# Generate a random API key for Typesense
resource "random_password" "typesense_api_key" {
  length  = 48
  special = false
}

# Typesense 
#
# Search index server instance
resource "aws_instance" "docbox_typesense" {
  // Canonical, Ubuntu, 24.04, arm64 noble image
  ami           = "ami-099eeb58169040255"
  instance_type = "t4g.small"
  subnet_id     = aws_subnet.private_subnet.id

  # Network security group
  vpc_security_group_ids = [aws_security_group.docbox_typesense_sg.id]

  # SSH key access
  key_name = aws_key_pair.ssh_key.key_name

  # Pass proxy details into setup script
  user_data = templatefile("../scripts/ec2-typesense-setup.sh", {
    proxy_host        = aws_instance.http_proxy.private_ip
    proxy_port        = "3128",
    typesense_api_key = random_password.typesense_api_key.result
  })

  # Disable running prolonged higher CPU speeds at a higher cost
  credit_specification {
    cpu_credits = "standard"
  }

  # Typesense must wait for the HTTP proxy to be fully initialized before
  # it can run so that it can use the HTTP proxy to install dependencies
  # (As it does not have regular network access since its in a private subnet)
  depends_on = [aws_instance.http_proxy]

  # Prevent replacement due to user_data changes
  lifecycle {
    ignore_changes = [user_data]
  }

  tags = {
    Name = "docbox-typesense"
  }
}



# HTTP Squid Proxy
# 
# Allows internal services from the private subnet to request HTTP
# resources from the public internet
resource "aws_instance" "http_proxy" {
  # Amazon Linux 2023 AMI 2023.7.20250527.1 arm64 HVM kernel-6.1
  ami           = "ami-0a06008c37dfe916b"
  instance_type = "t4g.nano"
  subnet_id     = aws_subnet.public_subnet.id

  # Network security group
  vpc_security_group_ids = [aws_security_group.http_proxy_sg.id]

  # SSH key access
  key_name = aws_key_pair.ssh_key.key_name

  user_data = file("../scripts/ec2-proxy-setup.sh")

  # Disable running prolonged higher CPU speeds at a higher cost
  credit_specification {
    cpu_credits = "standard"
  }

  tags = {
    Name = "docbox-http-proxy"
  }
}

