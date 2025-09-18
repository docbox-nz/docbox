ALTER TABLE "docbox_links"
ADD COLUMN "name_tsv" tsvector GENERATED ALWAYS AS (to_tsvector('english', "name")) STORED;

ALTER TABLE "docbox_folders"
ADD COLUMN "name_tsv" tsvector GENERATED ALWAYS AS (to_tsvector('english', "name")) STORED;

ALTER TABLE "docbox_files"
ADD COLUMN "name_tsv" tsvector GENERATED ALWAYS AS (to_tsvector('english', "name")) STORED;
