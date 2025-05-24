# Terraform S3 Bucket

> [!IMPORTANT]
> Terraform uses a state file to track the currently deployed infrastructure. In order to keep this in sync and backed up we will be using the S3 "backend" for terraform which will allow it to store the state in an S3 bucket so we don't have to manually share the state file.

# Create S3 bucket

Search for S3:

![Find S3](./images/setup-terraform/image.png)

Click "Create bucket":

![Create bucket](./images/setup-terraform/image-1.png)

Give the bucket a name:

![Set bucket name](./images/setup-terraform/image-2.png)

Ensure public access is blocked:

![No public access](./images/setup-terraform/image-3.png)

Enable bucket versioning:

> [!NOTE]
> Terraform recommends enabling bucket versioning 

![Bucket versioning](./images/setup-terraform/image-4.png)

Ensure encryption is enabled:

![Bucket encryption](./images/setup-terraform/image-5.png)

Create bucket:

![Create bucket](./images/setup-terraform/image-6.png)

View bucket details:

![View details](./images/setup-terraform/image-7.png)    

Copy bucket name:

![Bucket name](./images/setup-terraform/image-8.png)

You will use this bucket name for the `aws_infra_bucket` field when setting up the terraform config

After creating the bucket create a `s3.tfbackend` file in the `./terraform` folder in the project root.

In this file put the following:

```r
bucket =  "<THE BUCKET NAME ABOVE>"
key    = "docbox.tfstate"
region = "ap-southeast-2" # Or another region
profile = "<YOUR AWS PROFILE>"
```

With `<YOUR AWS PROFILE>` being your locally configured profile with credentials that have access to the bucket and creation of the various AWS infrastructure that terraform will need to create. 