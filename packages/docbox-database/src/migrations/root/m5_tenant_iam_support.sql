-- Allow nullability in the secret name
ALTER TABLE "docbox_tenants"
ALTER COLUMN "db_secret_name" DROP NOT NULL;

-- Add column to store the IAM specific db user username
ALTER TABLE "docbox_tenants"
ADD COLUMN "db_iam_user_name" VARCHAR NULL UNIQUE;
