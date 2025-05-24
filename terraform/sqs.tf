# Queue for file upload messages
resource "aws_sqs_queue" "docbox_queue" {
  name = "docbox-s3-upload-queue"

  tags = {
    Name = "docbox-http-proxy"
  }
}
