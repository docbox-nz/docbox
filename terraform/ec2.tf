# SSH public key for SSH access (EC2, VPN, PROXY)
resource "aws_key_pair" "ssh_key" {
  key_name   = "docbox_ssh_key"
  public_key = file(var.ssh_public_key_path)

  tags = {
    Name = "docbox-ssh-key"
  }
}

# Docbox API server EC2 
# (https://registry.terraform.io/providers/hashicorp/aws/latest/docs/data-sources/instance)
resource "aws_instance" "api" {
  ami           = var.ec2_image_ami
  instance_type = var.ec2_instance_class

  subnet_id = aws_subnet.private_subnet.id

  # SSH key access
  key_name = aws_key_pair.ssh_key.key_name

  # Network security group
  vpc_security_group_ids = [aws_security_group.docbox_sg.id]

  iam_instance_profile = aws_iam_instance_profile.docbox_instance_profile.name

  root_block_device {
    volume_type = var.ec2_storage_type
    volume_size = var.ec2_storage_size
  }

  # Disable running prolonged higher CPU speeds at a higher cost
  credit_specification {
    cpu_credits = "standard"
  }

  tags = {
    Name = "docbox-api"
  }
}

# HTTP Squid Proxy
# 
# Allows internal services from the private subnet to request HTTP
# resources from the public internet
resource "aws_instance" "http_proxy" {
  ami           = "ami-0809dd5035d9217b8" # Latest Amazon linux (08/08/2024)
  instance_type = "t3.nano"
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

