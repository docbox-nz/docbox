# Gateway to access the S3 service from our private subnet
resource "aws_vpc_endpoint" "s3_endpoint" {
  vpc_id            = var.vpc_id
  service_name      = "com.amazonaws.${var.aws_region}.s3"
  vpc_endpoint_type = "Gateway"

  route_table_ids = [aws_route_table.rt_private.id]
}

# Gateway to access the secrets manager service from our private subnet
resource "aws_vpc_endpoint" "secrets_endpoint" {
  vpc_id            = var.vpc_id
  service_name      = "com.amazonaws.${var.aws_region}.secretsmanager"
  vpc_endpoint_type = "Interface"

  subnet_ids         = [aws_subnet.private_subnet.id]
  security_group_ids = [aws_security_group.docbox_sg.id]

  private_dns_enabled = true
}

# Gateway to access the SQS queue service from our private subnet
resource "aws_vpc_endpoint" "sqs_endpoint" {
  vpc_id            = var.vpc_id
  service_name      = "com.amazonaws.${var.aws_region}.sqs"
  vpc_endpoint_type = "Interface"

  subnet_ids         = [aws_subnet.private_subnet.id]
  security_group_ids = [aws_security_group.docbox_sg.id]

  private_dns_enabled = true
}


# Public subnet "DMZ"
resource "aws_subnet" "public_subnet" {
  vpc_id                  = var.vpc_id
  cidr_block              = var.public_subnet_cidr
  map_public_ip_on_launch = true

  tags = {
    Name = "docbox-public-subnet"
  }
}

# Private subnet 
resource "aws_subnet" "private_subnet" {
  vpc_id            = var.vpc_id
  cidr_block        = var.private_subnet_cidr
  availability_zone = "${var.aws_region}a"

  tags = {
    Name = "docbox-private-subnet"
  }
}

# Create private route table
resource "aws_route_table" "rt_private" {
  vpc_id = var.vpc_id

  tags = {
    Name = "docbox-private-route-table"
  }
}

# Associate private route table and private subnet
resource "aws_route_table_association" "rt_private_assoc" {
  subnet_id      = aws_subnet.private_subnet.id
  route_table_id = aws_route_table.rt_private.id
}

# Create a route table for the public subnet
resource "aws_route_table" "rt_public" {
  vpc_id = var.vpc_id

  # Associate the internet gateway for outbound traffic
  route {
    cidr_block = "0.0.0.0/0"
    gateway_id = var.igw_id
  }

  tags = {
    Name = "docbox-public-route-table"
  }
}

# Associate the route table with the public subnet
resource "aws_route_table_association" "rta_public" {
  subnet_id      = aws_subnet.public_subnet.id
  route_table_id = aws_route_table.rt_public.id
}

