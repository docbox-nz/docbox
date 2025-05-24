ALTER TABLE "docbox_files" 
ADD COLUMN "parent_id" UUID
    CONSTRAINT "FK_file_file"
    REFERENCES "docbox_files" ("id")
    ON DELETE SET NULL;