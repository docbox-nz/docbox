
# Create instance profile to give the docbox EC2 instance the "docbox" role
resource "aws_iam_instance_profile" "docbox_instance_profile" {
  name = "docbox_instance_profile"
  role = aws_iam_role.docbox_role.name
}

# Role for the docbox API instance
resource "aws_iam_role" "docbox_role" {
  name = "docbox_role"

  assume_role_policy = jsonencode({
    Version = "2012-10-17"
    Statement = [{
      Effect = "Allow"
      Principal = {
        Service = "ec2.amazonaws.com"
      }
      Action = "sts:AssumeRole"
    }]
  })
}

# IAM Policy that allows the docbox role access to retrieve the following secrets:
# - Docbox development postgres secrets
# - Docbox production postgres secrets
# - Docbox config secrets
resource "aws_iam_policy" "docbox_secrets_manager_policy" {
  name        = "docbox_secrets_access_policy"
  description = "Allow access to per tenant database and docbox database credentials"

  policy = jsonencode({
    Version = "2012-10-17",
    Statement = [{
      Effect = "Allow",
      Action = [
        "secretsmanager:GetSecretValue",
      ],
      Resource = [
        # Per tenant individual database user credentials
        "arn:aws:secretsmanager:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:secret:postgres/docbox/dev/*",
        "arn:aws:secretsmanager:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:secret:postgres/docbox/prod/*",
        # Root docbox database user credentials
        "arn:aws:secretsmanager:${data.aws_region.current.name}:${data.aws_caller_identity.current.account_id}:secret:postgres/docbox/config*",
      ]
    }]
  })
}

# Attach the "docbox_secrets_manager_policy" policy to the docbox role
resource "aws_iam_role_policy_attachment" "docbox_secrets_manager_policy_attachment" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.docbox_secrets_manager_policy.arn
}


# IAM Policy that allows the docbox role to perform the following actions on S3 scoped to docbox-* buckets:
# - Upload files
# - Get files
# - Delete files
resource "aws_iam_policy" "docbox_s3_access_policy" {
  name        = "docbox_s3_access_policy"
  description = "Allows S3 access to freely modify any buckets prefixed with docbox- for the docbox EC2"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      # Object level actions
      {
        Effect = "Allow"
        Action = [
          "s3:PutObject",
          "s3:GetObject",
          "s3:DeleteObject"
        ]
        Resource = [
          "arn:aws:s3:::docbox-*/*"
        ]
      }
    ]
  })
}

# Attach the "docbox_secrets_manager_policy" policy to the docbox role
resource "aws_iam_role_policy_attachment" "docbox_s3_access_attachment" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.docbox_s3_access_policy.arn
}

# Policy that allows subscribing to S3 notifications from the SQS queue
resource "aws_iam_policy" "docbox_sqs_read" {
  name        = "sqs_s3_notification_policy"
  description = "Allow docbox EC2 to receive S3 notifications from SQS"

  # The policy document allowing EC2 to read messages from the SQS queue
  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Effect = "Allow"
        Action = [
          "SQS:ReceiveMessage",
          "SQS:DeleteMessage",
          "SQS:GetQueueAttributes"
        ]
        Resource = aws_sqs_queue.docbox_queue.arn
      }
    ]
  })
}

# Attach the "docbox_sqs_read" policy to the docbox role
resource "aws_iam_role_policy_attachment" "docbox_role_sqs_policy" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.docbox_sqs_read.arn
}

# Policy on the docbox S3 notification SQS queue that permits AWS S3
# to push new messages onto the queue
resource "aws_sqs_queue_policy" "docbox_s3_sqs_policy" {
  queue_url = aws_sqs_queue.docbox_queue.id

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      {
        Sid    = "docbox-queue-events"
        Effect = "Allow"
        Principal = {
          Service = "s3.amazonaws.com"
        }
        Action   = "SQS:SendMessage"
        Resource = aws_sqs_queue.docbox_queue.arn
        Condition = {
          ArnLike = {
            "aws:SourceArn" = "arn:aws:s3:::docbox-*"
          }
        }
      }
    ]
  })
}
