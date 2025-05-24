CREATE TABLE "docbox_generated_files"
(
    "id"         UUID                     NOT NULL
        PRIMARY KEY,
    "file_id"    UUID                     NOT NULL
        CONSTRAINT "FK_generated_file_file"
            REFERENCES "docbox_files" ("id")
            ON DELETE RESTRICT,
    "mime"       VARCHAR                  NOT NULL,
    "type"       TEXT                     NOT NULL,
    "hash"       VARCHAR                  NOT NULL,
    "file_key"   VARCHAR                  NOT NULL,
    "created_at" TIMESTAMP WITH TIME ZONE NOT NULL
);
