-- Ensure the pg_trgm extension is enabled
CREATE EXTENSION IF NOT EXISTS pg_trgm;

-- Index existing name fields
CREATE INDEX idx_docbox_folders_name ON "docbox_folders"  USING gin ("name" gin_trgm_ops);
CREATE INDEX idx_docbox_files_name ON "docbox_files" USING gin ("name" gin_trgm_ops);
CREATE INDEX idx_docbox_links_name ON "docbox_links" USING gin ("name" gin_trgm_ops);

-- Index link values
CREATE INDEX idx_docbox_links_value ON "docbox_links" USING gin ("value" gin_trgm_ops);
