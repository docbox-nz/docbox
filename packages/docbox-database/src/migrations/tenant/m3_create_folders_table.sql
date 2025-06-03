CREATE TABLE "docbox_folders"
(
    "id"           UUID                     NOT NULL
        PRIMARY KEY,
    "name"         VARCHAR                  NOT NULL,
    "document_box" VARCHAR                  NOT NULL
        CONSTRAINT "FK_folders_document_box"
            REFERENCES "docbox_boxes" ("scope")
            ON DELETE RESTRICT,
    "folder_id"    UUID
        CONSTRAINT "FK_folders_folder"
            REFERENCES "docbox_folders" ("id")
            ON DELETE RESTRICT,
    "created_at"   TIMESTAMP WITH TIME ZONE NOT NULL,
    "created_by"   VARCHAR
        CONSTRAINT "FK_folders_created_by"
            REFERENCES "docbox_users" ("id")
            ON DELETE RESTRICT
);

CREATE INDEX idx_folders_folder_id
ON "docbox_folders" ("folder_id");

CREATE INDEX idx_folders_document_box_folder_id
ON "docbox_folders" ("document_box", "folder_id");