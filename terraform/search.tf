
# Opensearch index 
# (https://registry.terraform.io/providers/hashicorp/aws/latest/docs/resources/opensearch_domain)
resource "aws_opensearch_domain" "opensearch" {
  domain_name    = var.opensearch_domain
  engine_version = "OpenSearch_2.13" # Latest version of opensearch (As at 05/08/2024)

  cluster_config {
    instance_type  = var.opensearch_instance_class
    instance_count = var.opensearch_instance_count
  }

  # Storage Options
  ebs_options {
    ebs_enabled = true
    # Per node volume size
    volume_size = var.opensearch_volume_size
    volume_type = var.opensearch_volume_type
  }

  #  Encrypt the search index at rest
  encrypt_at_rest {
    enabled = true
  }

  # Encrypt connections between nodes
  node_to_node_encryption {
    enabled = true
  }


  domain_endpoint_options {
    # Enforce HTTPs communication
    enforce_https = true

    # Enforce modern TLS requirements
    tls_security_policy = "Policy-Min-TLS-1-2-PFS-2023-10"
  }

  # Setup access credentials
  advanced_security_options {
    enabled                        = false
    anonymous_auth_enabled         = true
    internal_user_database_enabled = true

    master_user_options {
      master_user_name     = var.opensearch_username
      master_user_password = var.opensearch_password
    }
  }

  # Setup access policies
  access_policies = data.aws_iam_policy_document.opensearch_policy_doc.json

  # Only allow access through VPC
  vpc_options {
    # Make part of the security group to allow access 
    security_group_ids = [aws_security_group.opensearch_sg.id]

    subnet_ids = [aws_subnet.private_subnet.id]
  }

  tags = {
    Domain = "docbox-search"
    Name   = "docbox-search"
  }

  # depends_on = [aws_iam_service_linked_role.opensearch_linked_role]
}
