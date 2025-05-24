terraform {
  required_providers {
    aws = {
      source  = "hashicorp/aws"
      version = "~> 5.61"
    }
  }

  required_version = ">= 1.2.0"

  # Use an AWS S3 bucket to store and manage the terraform state
  backend "s3" {}
}

provider "aws" {
  region  = var.aws_region
  profile = var.aws_profile
}


data "aws_region" "current" {}

data "aws_caller_identity" "current" {}

