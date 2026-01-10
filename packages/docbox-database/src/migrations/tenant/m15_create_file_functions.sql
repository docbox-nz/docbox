-- ================================================================
-- Resolve the path of a file by ID
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_file_path(p_file_id UUID)
RETURNS TABLE (id UUID, name VARCHAR)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    SELECT "id", "name", "folder_id", 0 AS "depth"
    FROM "docbox_files"
    WHERE "docbox_files"."id" = p_file_id
    UNION ALL (
        SELECT
            "folder"."id",
            "folder"."name",
            "folder"."folder_id",
            "folder_hierarchy"."depth" + 1 as "depth"
        FROM "docbox_folders" AS "folder"
        INNER JOIN "folder_hierarchy" ON "folder"."id" = "folder_hierarchy"."folder_id"
    )
)
SELECT "folder_hierarchy"."id", "folder_hierarchy"."name"
FROM "folder_hierarchy"
WHERE "folder_hierarchy"."id" <> p_file_id
ORDER BY "folder_hierarchy"."depth" DESC
$$;

-- ================================================================
-- Resolve a collection of file paths for `p_file_ids` within
-- `p_document_box`
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_files_paths(
    p_document_box VARCHAR,
    p_file_ids UUID[]
)
RETURNS TABLE (item_id UUID, path JSONB)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    -- Start from the parent folder to avoid including the folder itself
    SELECT
        "file"."id" AS "item_id",
        "parent"."folder_id" AS "parent_folder_id",
        0 AS "depth",
        jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) AS "path"
    FROM "docbox_files" "file"
    JOIN docbox_folders "parent" ON "file"."folder_id" = "parent"."id"
    WHERE "file"."id" = ANY(p_file_ids)
        AND "parent"."document_box" = p_document_box

    UNION ALL

    SELECT
        "fh"."item_id",
        "parent"."folder_id",
        "fh"."depth" + 1,
        jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
    FROM "folder_hierarchy" "fh"
    JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
),
"folder_paths" AS (
    SELECT "item_id", "path", ROW_NUMBER() OVER (PARTITION BY "item_id" ORDER BY "depth" DESC) AS "rn"
    FROM "folder_hierarchy"
)
SELECT "item_id", "path"
FROM "folder_paths"
-- Only take the parent entries
WHERE "rn" = 1
$$;


-- ================================================================
-- Resolve folders by IDs within a single scope with extra
-- additional data like folder path and the user details for last
-- modified and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_files_with_extra(
    p_document_box VARCHAR,
    p_file_ids UUID[]
)
RETURNS TABLE (
    file docbox_file,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path JSONB
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_file("file") AS "file",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path"
    FROM "docbox_files" AS "file"
    LEFT JOIN "docbox_users" AS "cu"
        ON "file"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "file"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_file" AS "ehl"
        ON "file"."id" = "ehl"."file_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN resolve_files_paths(p_document_box, p_file_ids) AS "fp"
        ON "file".id = "fp"."item_id"
    WHERE "file"."id" = ANY(p_file_ids)
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolve file by ID within a single scope with extra additional
-- data like the folder path and the user details for last modified
-- and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_file_by_id_with_extra(
    p_document_box VARCHAR,
    p_file_id UUID
)
RETURNS TABLE (
    file docbox_file,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_file("file") AS "file",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_files" AS "file"
    LEFT JOIN "docbox_users" AS "cu"
        ON "file"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "file"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_file" AS "ehl"
        ON "file"."id" = "ehl"."file_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "file"."id" = p_file_id
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolves all files within a folder with extra additional data
-- like the creator and the last modification details
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_files_by_parent_folder_with_extra(p_parent_id UUID)
RETURNS TABLE (
    file docbox_file,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_file("file") AS "file",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_files" AS "file"
    LEFT JOIN "docbox_users" AS "cu"
        ON "file"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "file"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_file" AS "ehl"
        ON "file"."id" = "ehl"."file_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "file"."folder_id" = p_parent_id
$$;

-- ================================================================
-- Resolves all children files of a file with extra additional data
-- like the creator and the last modification details
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_files_by_parent_file_with_extra(p_parent_id UUID)
RETURNS TABLE (
    file docbox_file,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_file("file") AS "file",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_files" AS "file"
    LEFT JOIN "docbox_users" AS "cu"
        ON "file"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "file"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_file" AS "ehl"
        ON "file"."id" = "ehl"."file_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "file"."parent_id" = p_parent_id
$$;


-- ================================================================
-- Resolve files by scope and ID pairs with extra additional data
-- like the folder path and the user details for last modified and
-- creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_files_with_extra_mixed_scopes(
    p_input docbox_input_pair[]
)
RETURNS TABLE (
    file docbox_file,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path JSONB,
    document_box VARCHAR
)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE
        "input_files" AS (
            SELECT document_box, file_id
            FROM UNNEST(p_input) AS t(document_box, file_id)
        ),
        "folder_hierarchy" AS (
            SELECT
                "file"."id" AS "item_id",
                "parent"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) AS "path"
            FROM "docbox_files" "file"
            JOIN "input_files" "i" ON "file"."id" = "i"."file_id"
            JOIN "docbox_folders" "parent" ON "file"."folder_id" = "parent"."id"
            WHERE "parent"."document_box" = "i"."document_box"

            UNION ALL

            SELECT
                "fh"."item_id",
                "parent"."folder_id",
                "fh"."depth" + 1,
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) || "fh"."path"
            FROM "folder_hierarchy" "fh"
            JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
        ),
        "folder_paths" AS (
            SELECT "item_id", "path", ROW_NUMBER() OVER (PARTITION BY "item_id" ORDER BY "depth" DESC) AS "rn"
            FROM "folder_hierarchy"
        )
    SELECT
        mk_docbox_file("file") AS "file",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path",
        "folder"."document_box" AS "document_box"
    FROM "docbox_files" AS "file"
    LEFT JOIN "docbox_users" AS "cu"
        ON "file"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "file"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_file" AS "ehl"
        ON "file"."id" = "ehl"."file_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN "folder_paths" "fp"
        ON "file".id = "fp"."item_id" AND "fp".rn = 1
    JOIN "input_files" "i"
        ON "file"."id" = "i"."file_id"
    WHERE "folder"."document_box" = "i"."document_box"
$$;
