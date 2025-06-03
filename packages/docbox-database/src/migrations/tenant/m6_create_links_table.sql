CREATE TABLE "docbox_links"
(
    "id"         UUID                     NOT NULL
        PRIMARY KEY,
    "name"       VARCHAR                  NOT NULL,
    "value"      VARCHAR                  NOT NULL,
    "folder_id"  UUID
        CONSTRAINT "FK_link_folder"
            REFERENCES "docbox_folders" ("id")
            ON DELETE RESTRICT,
    "created_at" TIMESTAMP WITH TIME ZONE NOT NULL,
    "created_by" VARCHAR
        CONSTRAINT "FK_link_created_by"
            REFERENCES "docbox_users" ("id")
            ON DELETE RESTRICT
);

CREATE INDEX idx_links_folder_id 
ON "docbox_links" ("folder_id");