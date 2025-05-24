
# Terraform initial setup

You will need to install terraform you can find the various install and download guides [Here](https://developer.hashicorp.com/terraform/install?product_intent=terraform)

## 1. Setup SSH Key

The terraform scripts depend on you having a SSH key for accessing the EC2 instances. This is using during the initial setup and will also be the keys you use later to SSH into the systems.

- `~/.ssh/docbox.pub` (The public key)
- `~/.ssh/docbox` (The private key)

Theses keys can be named anything but keep them in an accessible place as you will need to set them as the following variables in a later step:

```t
# SSH keys
ssh_public_key_path  = "~/.ssh/docbox.pub"
ssh_private_key_path = "~/.ssh/docbox"
```

## 2. Create opensearch role

For first time setup with AWS to setup open search you must run the following command to create
a linked role:

```sh
aws iam create-service-linked-role --aws-service-name es.amazonaws.com --profile <your-aws-profile>
```

> You only need to do this the first time you setup


## 3. Setup API AMI

Our Debian image needs to have libreoffice and a few other dependencies installed which we can't get while on the private subnet without allocating an EIP and NAT so instead we can create a public EC2 instance -> setup our deps with internet access -> create an AMI image and then use that for our private container.

To setup the instructions for doing this are [Here](./SETUP_AMI.md)

## 4. Terraform State

> [!IMPORTANT]
> Terraform uses a state file to track the currently deployed infrastructure. In order to keep this in sync and backed up we will be using the S3 "backend" for terraform which will allow it to store the state in an S3 bucket so we don't have to manually share the state file.

You will need to follow [Setup Terraform S3](./SETUP_TERRAFORM_S3.md) to setup a bucket to store the state. The same bucket can be reused for multiple infrastructures as long as the key used in the terraform config is different

## 5. Initialize Terraform

Run the init command

```sh
terraform -chdir=./terraform init -backend-config=s3.tfbackend
```

## 6. Terraform config

You will need a terraform config file named `terraform.tfvars` and it should be placed in the `terraform` folder.
It should include the following:

```t
# AWS configuration
aws_profile = "YOUR AWS PROFILE"
aws_region  = "ap-southeast-2"  

vpc_id = "ID OF THE TARGET VPC"
vpc_cidr = "CIDR BLOCK OF VPC"

# Subnet & VPC configuration (!!Ensure these are different from existing ones!!)
private_subnet_cidr   = "10.0.1.0/24"
public_subnet_cidr    = "10.0.3.0/24"

# SSH keys (Update theses paths to be the paths to your SSH key files)
ssh_public_key_path  = "~/.ssh/docbox.pub"
ssh_private_key_path = "~/.ssh/docbox"

# Open search configuration
opensearch_domain = "docbox-search"
opensearch_instance_class = "t3.medium.search"
opensearch_volume_size    = 10
opensearch_volume_type    = "gp3"
opensearch_username       = "USERNAME FOR OPENSEARCH"
opensearch_password       = "PASSWORD ACCESSING OPENSEARCH"
opensearch_instance_count = 1

# EC2 Instance
ec2_image_ami      = "AMI FOR DOCBOX FROM ABOVE"
ec2_instance_class = "t3.micro"
ec2_storage_size   = 8
ec2_storage_type   = "gp3"
```

# Infrastructure deploy

After the terraform initial setup is complete you can deploy the infrastructure using terraform. You can
run this any time you change a part of the infrastructure.

## 1. Terraform apply

You can deploy the infrastructure using the terraform apply command:

```sh
terraform -chdir=./terraform apply
```

> This will create and update all the resources for this project on AWS

## 2. Allow database access

1. Visit the RDS page on the AWS console, 
2. Find the database
3. Visit its security group 
4. Edit inbound rules
5. Add an allow rule for the docbox security group


## 3. Update route table

Go to "VPC" in the AWS console then "Route Tables" and find "docbox-private-route-table" click edit routes and add a new route
for "{DESTINATION}/16" (The CIDR block for the subnet containing your main API VPC) from the "Target" dropdown select "Peering Connection"
and select the peering connection then save the changes

## 4. Setup EC2

> [!IMPORTANT]
>
> You will need to be connected to a VPN that can access the private subnet 
> in order to perform these actions

You can deploy the setup script to the EC2 instance:

```sh
sh ./scripts/deploy.sh --setup
```

> On windows use git-bash to run the shell scripts

This will copy the `./scripts/ec2-docbox-setup.sh` script to the EC2 instance and run the script
which will update the system and install all required dependencies.

## 4. Environment variables

You will need to create a `.env.production` file in the project root. It should contain the following:

```sh
# Logging configuration
RUST_LOG=aws_sdk_secretsmanager=info,aws_runtime=info,aws_smithy_runtime=info,hyper_util=info,debug

# Address to bind the server on
SERVER_ADDRESS=0.0.0.0:8080

# URL for the office converter server (This should be running on the same server)
CONVERT_SERVER_ADDRESS=http://localhost:8081

# URL for the SQS queue for notifications of when a new file is created
DOCBOX_SQS_URL={SQS QUEUE ARN}

# Database host and port
POSTGRES_HOST={DATABASE HOST}
POSTGRES_PORT={DATABASE PORT}

# AWS secrets manager key for the root database username and password
DOCBOX_DB_CREDENTIAL_NAME=postgres/docbox/config

# OpenSearch instance URL
OPENSEARCH_URL=https://{OPENSEARCH_USERNAME}:{OPENSEARCH_PASSWORD}@{OPENSEARCH_HOST}
# OpenSearch ML model if using nueral searches
OPENSEARCH_MODEL_ID={OPENSEARCH_MODEL_ID}

# URL for the HTTP proxy server to use for scraping
HTTP_PROXY=http://{PROXY SERVER IP}:3128/

# AWS region for AWS services to use
AWS_REGION=ap-southeast-2
```

You can then deploy this to the EC2 instance using (Use this anytime you want to upload the env to the EC2 instance)

```sh
sh ./scripts/deploy.sh --env
```

> On windows use git-bash to run the shell scripts
>
> i.e `git-bash` then run the command from the git-bash window that opens

## 5. Deploy binary

You must first build the rust binary with:

**Linux**:

```sh
cargo build -p docbox --target x86_64-unknown-linux-musl --release
```

**Windows**:

```sh
cross build -p docbox --target x86_64-unknown-linux-musl --release
```

On windows setup with [Build Windows](./BUILD_WINDOWS.md) first and use the build command in from there,

Then you can run which will copy the file to the server and restart the service:

```sh
sh ./scripts/deploy.sh --binary
```

> On windows use git-bash to run the shell scripts
>
> i.e `git-bash` then run the command from the git-bash window that opens

The server will now be running and accessible through the VPC peer route

# Plan Infrastructure Changes

(Shows you what will change without changing it)

```sh
terraform -chdir=./terraform plan
```


# Destroy Infrastructure

To remove all infrastructure run 

```sh
terraform -chdir=./terraform destroy
```

> [!IMPORTANT]
> You will need to manually delete things such as RDS databases and empty S3 bucket contents and delete S3 buckets. As terraform will not be able to delete this.
>
> I recommend starting the database delete before running the destroy command as the deletion requires creating a database snapshot so the terraform destroy will fail and you'll have to run it again after deleting the database to fully destroy.
>
> S3 buckets created by docbox are not destroyed by terraform as terraform is not aware of them, thus they will need to be emptied and destroyed manually

