
# Create credentials file

The credentials file is used by the CLI to access the database with a higher level of permissions to perform things like migrations and setting up tenants. You will need this to perform a majority of the CLI actions:


Create a file name `cli-credentials.json` in the root of the repository:

```json
{
    "host": "{DATABASE HOST}",
    "port": 5432, // DATABASE PORT
    "username": "DATABASE USERNAME",
    "password": "DATABASE PASSWORD"
}
```

# Initialize root database

To initialize the root database for docbox run the following command

```sh
cargo run --release -p docbox-cli -- create-root
```

# Create new tenant

Create a new file in this case `demo-tenant.json` this will contain the following:

```json
{
    "id": "00000000-0000-0000-0000-000000000000",
    "env": "{Development/Production}",
    "db_name": "docbox-{tag}-{dev/prod}",
    "db_secret_name": "postgres/docbox/{dev/prod}/{tag}",
    "db_role_name": "{TENANT DB ROLE NAME}",
    "db_password": "{TENANT DB ROLE PASSWORD}",
    "s3_name": "docbox-{tag}-{dev/prod}",
    "os_index_name": "docbox-{tag}-{dev/prod}",
    "s3_queue_arn": "arn:aws:sqs:ap-southeast-2:{YOUR_S3_UPLOADS_QUEUE_ARN}",
    "event_queue_url": "https://sqs.ap-southeast-2.amazonaws.com/{YOUR_EVENT_QUEUE_ARN}",
    "origins": [
        "https://{SOME_ORIGIN}"
    ]
}
```

Then run the following command from the root of this repository:

> You must ensure you have a `.env` file setup containing all the required environment 
> variables from the terraform setup
>
> Along with:
> ```
> AWS_ACCESS_KEY_ID={YOUR AWS KEY ID}
> AWS_SECRET_ACCESS_KEY={YOUR AWS SECRET ACCESS KEY}
> ```
>
> The specified key must have high enough permission to manage S3 buckets
> and access opensearch


```sh
cargo run --release -p docbox-cli -- create-tenant --file ./demo-tenant.json
```

# Migrations

To be documented, but runs a SQL migration over the database (With optional filtering for environment or tenant) 

```sh
 cargo run --release -p docbox-cli -- migrate --env Development --file ./packages/docbox-cli/migrations/m1_file_parent_id.sql --tenant-id e3bab7bd-07a5-4b81-be38-e4790e80c0d1
```