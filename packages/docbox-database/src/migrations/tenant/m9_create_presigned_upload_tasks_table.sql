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
    "created_by" VARCHAR,
    "parent_id" UUID
        CONSTRAINT "FK_presigned_task_file"
            REFERENCES "docbox_files" ("id")
            ON DELETE SET NULL,
    "processing_config" JSONB
);

