-- ================================================================
-- Composite types
-- ================================================================

CREATE TYPE docbox_folder AS (
    "id" UUID,
    "name" VARCHAR,
    "pinned" BOOLEAN,
    "document_box" VARCHAR,
    "folder_id" UUID,
    "created_at" TIMESTAMP WITH TIME ZONE,
    "created_by" VARCHAR
);

CREATE TYPE docbox_file AS (
    "id" UUID,
    "name" VARCHAR,
    "mime" VARCHAR,
    "folder_id" UUID,
    "parent_id" UUID,
    "hash" VARCHAR,
    "size" INTEGER,
    "encrypted" BOOLEAN,
    "pinned" BOOLEAN,
    "file_key" VARCHAR,
    "created_at" TIMESTAMP WITH TIME ZONE,
    "created_by" VARCHAR
);

CREATE TYPE docbox_link AS (
    "id" UUID,
    "name" VARCHAR,
    "value" VARCHAR,
    "pinned" BOOLEAN,
    "folder_id" UUID,
    "created_at" TIMESTAMP WITH TIME ZONE,
    "created_by" VARCHAR
);

CREATE TYPE docbox_user AS (
    "id" VARCHAR,
    "name" VARCHAR,
    "image_id" VARCHAR
);

CREATE TYPE docbox_input_pair AS (
    "scope" TEXT,
    "id" UUID
);

-- ================================================================
-- Views to query the latest edit history on a per  item type basis
-- ================================================================

CREATE VIEW "docbox_latest_edit_per_file" AS
SELECT DISTINCT ON ("file_id") "file_id", "user_id", "created_at"
FROM "docbox_edit_history"
ORDER BY "file_id", "created_at" DESC;

CREATE VIEW "docbox_latest_edit_per_folder" AS
SELECT DISTINCT ON ("folder_id") "folder_id", "user_id", "created_at"
FROM "docbox_edit_history"
ORDER BY "folder_id", "created_at" DESC;

CREATE VIEW "docbox_latest_edit_per_link" AS
SELECT DISTINCT ON ("link_id") "link_id", "user_id", "created_at"
FROM "docbox_edit_history"
ORDER BY "link_id", "created_at" DESC;

-- ================================================================
-- Helper function to construct a user or null, used in places
-- where the user may be null and we want to short circuit instead
-- of creating a docbox_user filled with nulls
-- ================================================================

CREATE FUNCTION mk_docbox_user(p_user docbox_users)
RETURNS docbox_user
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT CASE
        WHEN p_user IS NULL THEN NULL
        ELSE ROW(p_user."id", p_user."name", p_user."image_id")::docbox_user
    END
$$;

COMMENT ON FUNCTION mk_docbox_user(docbox_users)
IS 'Helper to construct a docbox_user from a docbox_users row handling the null case to prevent constructing a user of NULL only values';

-- ================================================================
-- Helper function to construct a docbox_folder from a row of the
-- docbox_folders table
-- ================================================================

CREATE FUNCTION mk_docbox_folder(p_folder docbox_folders)
RETURNS docbox_folder
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT ROW(
        p_folder."id",
        p_folder."name",
        p_folder."pinned",
        p_folder."document_box",
        p_folder."folder_id",
        p_folder."created_at",
        p_folder."created_by"
    )::docbox_folder
$$;

COMMENT ON FUNCTION mk_docbox_folder(docbox_folders)
IS 'Helper to construct a docbox_folder from a docbox_folders row';

-- ================================================================
-- Helper function to construct a docbox_file from a row of the
-- docbox_files table
-- ================================================================

CREATE FUNCTION mk_docbox_file(p_file docbox_files)
RETURNS docbox_file
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT ROW(
        p_file."id",
        p_file."name",
        p_file."mime",
        p_file."folder_id",
        p_file."parent_id",
        p_file."hash",
        p_file."size",
        p_file."encrypted",
        p_file."pinned",
        p_file."file_key",
        p_file."created_at",
        p_file."created_by"
    )::docbox_file
$$;

COMMENT ON FUNCTION mk_docbox_file(docbox_files)
IS 'Helper to construct a docbox_file from a docbox_files row';

-- ================================================================
-- Helper function to construct a docbox_link from a row of the
-- docbox_links table
-- ================================================================

CREATE FUNCTION mk_docbox_link(p_link docbox_links)
RETURNS docbox_link
LANGUAGE sql
IMMUTABLE
AS $$
    SELECT ROW(
        p_link."id",
        p_link."name",
        p_link."value",
        p_link."pinned",
        p_link."folder_id",
        p_link."created_at",
        p_link."created_by"
    )::docbox_link
$$;

COMMENT ON FUNCTION mk_docbox_link(docbox_links)
IS 'Helper to construct a docbox_link from a docbox_links row';
