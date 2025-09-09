CREATE TABLE "docbox_files_pages"
(
    "file_id"      UUID NOT NULL,

    "page"         INTEGER NOT NULL,

    "content"      TEXT NOT NULL,

    "content_tsv"  tsvector GENERATED ALWAYS AS (to_tsvector('english', "content")) STORED,

    CONSTRAINT "PK_file_page" PRIMARY KEY ("file_id", "page")
);
