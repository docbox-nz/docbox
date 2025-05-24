CREATE TABLE IF NOT EXISTS "docbox_presigned_upload_tasks" (
    "id" UUID PRIMARY KEY,
    "status" JSONB,
    "name" VARCHAR NOT NULL,
    "mime" VARCHAR NOT NULL,
    "size" INTEGER NOT NULL,
    "document_box" VARCHAR NOT NULL,
    "folder_id" UUID,
    "file_key" VARCHAR NOT NULL,
    "created_at" timestamp with time zone NOT NULL,
    "expires_at" timestamp with time zone NOT NULL,
    "created_by" VARCHAR
);