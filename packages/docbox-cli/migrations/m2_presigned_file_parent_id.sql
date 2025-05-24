-- Add the parent ID to the table
ALTER TABLE "docbox_presigned_upload_tasks" 
ADD COLUMN "parent_id" UUID
    CONSTRAINT "FK_presigned_task_file"
    REFERENCES "docbox_files" ("id")
    ON DELETE SET NULL;