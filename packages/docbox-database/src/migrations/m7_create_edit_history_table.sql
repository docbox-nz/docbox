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
