CREATE TABLE "docbox_files"
(
    "id"         UUID                     NOT NULL
        PRIMARY KEY,
    "name"       VARCHAR                  NOT NULL,
    "mime"       VARCHAR                  NOT NULL,
    "folder_id"  UUID
        CONSTRAINT "FK_file_folder"
            REFERENCES "docbox_folders" ("id")
            ON DELETE RESTRICT,    
    "parent_id"  UUID
        CONSTRAINT "FK_file_file"
            REFERENCES "docbox_files" ("id")
            ON DELETE SET NULL,
    "hash"       VARCHAR                  NOT NULL,
    "size"       INTEGER                  NOT NULL,
    "encrypted"  BOOLEAN DEFAULT FALSE,
    "file_key"   VARCHAR                  NOT NULL,
    "created_at" TIMESTAMP WITH TIME ZONE NOT NULL,
    "created_by" VARCHAR
        CONSTRAINT "FK_file_created_by"
            REFERENCES "docbox_users" ("id")
            ON DELETE RESTRICT
);
