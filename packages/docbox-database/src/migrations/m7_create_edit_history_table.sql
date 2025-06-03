CREATE TABLE IF NOT EXISTS "docbox_edit_history"
(
    "id"         UUID                     NOT NULL
        PRIMARY KEY,
    "file_id"    UUID
        CONSTRAINT "FK_edit_history_file"
            REFERENCES "docbox_files" ("id")
            ON DELETE CASCADE,
    "link_id"    UUID
        CONSTRAINT "FK_edit_history_link"
            REFERENCES "docbox_links" ("id")
            ON DELETE CASCADE,
    "folder_id"  UUID
        CONSTRAINT "FK_edit_history_folder"
            REFERENCES "docbox_folders" ("id")
            ON DELETE CASCADE,
    "user_id"    VARCHAR
        CONSTRAINT "FK_edit_history_user"
            REFERENCES "docbox_users" ("id")
            ON DELETE CASCADE,
    "type"       TEXT                     NOT NULL,
    "metadata"   JSONB                    NOT NULL,
    "created_at" TIMESTAMP WITH TIME ZONE NOT NULL
);

-- Index file results for fast latest history
CREATE INDEX idx_edit_history_file_created_at_desc
ON "docbox_edit_history" ("file_id", "created_at" DESC);

-- Index folder results for fast latest edit history
CREATE INDEX idx_edit_history_folder_created_at_desc
ON "docbox_edit_history" ("folder_id", "created_at" DESC);

-- Index link results for fast latest edit history
CREATE INDEX idx_edit_history_link_created_at_desc
ON "docbox_edit_history" ("link_id", "created_at" DESC);