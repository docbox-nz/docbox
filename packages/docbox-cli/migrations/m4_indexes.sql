-- Indexes for edit history 

CREATE INDEX idx_edit_history_file_created_at_desc
ON "docbox_edit_history" ("file_id", "created_at" DESC);

CREATE INDEX idx_edit_history_folder_created_at_desc
ON "docbox_edit_history" ("folder_id", "created_at" DESC);

CREATE INDEX idx_edit_history_link_created_at_desc
ON "docbox_edit_history" ("link_id", "created_at" DESC);

-- Indexes for parent folder

CREATE INDEX idx_links_folder_id 
ON "docbox_links" ("folder_id");

CREATE INDEX idx_files_folder_id 
ON "docbox_files" ("folder_id");

CREATE INDEX idx_folders_folder_id 
ON "docbox_folders" ("folder_id");

-- Index generated files

CREATE INDEX idx_generated_files_file_id
ON "docbox_generated_files" ("file_id", "type");

-- Index document box and folder ID for faster root folder lookups

CREATE INDEX idx_folders_document_box_folder_id
ON "docbox_folders" ("document_box", "folder_id");