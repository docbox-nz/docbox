-- Create index for fast lookups via the storage bucket name (Presigned uploads)
CREATE INDEX IF NOT EXISTS "idx_docbox_tenants_storage_bucket_name"
ON "docbox_tenants" ("s3_name");
