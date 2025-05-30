# IAM access policies, roles and configuration to allow
# the API EC2 instance to work with S3 buckets


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

# IAM policy to allow docbox to access the secrets manger
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

# Create attachment between policy
resource "aws_iam_role_policy_attachment" "docbox_secrets_manager_policy_attachment" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.docbox_secrets_manager_policy.arn
}


# IAM Policy to allow S3 access to the API EC2 
resource "aws_iam_policy" "s3_access_policy" {
  name        = "docbox_s3_access_policy"
  description = "Allows S3 access to freely modify any buckets prefixed with docbox- for the docbox EC2"

  policy = jsonencode({
    Version = "2012-10-17"
    Statement = [
      # Bucket level actions
      {
        Effect = "Allow"
        Action = [
          "s3:CreateBucket",
          "s3:ListBucket",
          "s3:DeleteBucket",
          "s3:PutBucketNotification",
          "s3:PutBucketCORS"
        ]
        Resource = [
          "arn:aws:s3:::docbox-*"
        ]
      },
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

# Create attachment between the s3 access role and policy
resource "aws_iam_role_policy_attachment" "s3_access_attachment" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.s3_access_policy.arn
}

# Create instance profile to give the EC2 instance the S3 access role
resource "aws_iam_instance_profile" "docbox_instance_profile" {
  name = "docbox_instance_profile"
  role = aws_iam_role.docbox_role.name
}

# Policy that allows subscribing to s3 notifications
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

# Attach sqs read policy to docbox role
resource "aws_iam_role_policy_attachment" "docbox_role_sqs_policy" {
  role       = aws_iam_role.docbox_role.name
  policy_arn = aws_iam_policy.docbox_sqs_read.arn
}

# Role for the S3 buckets
resource "aws_iam_role" "s3_bucket_role" {
  name = "s3_bucket_role"

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

# Policy allowing docbox s3 buckets to notify the SQS queue
resource "aws_sqs_queue_policy" "sqs_policy" {
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
