-- ================================================================
-- Resolve the ID and names of the parent folder tree using the ID
-- of the desired folder.
--
-- Recursively walks up the folder parent tree collecting a table
-- of the parents
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folder_path(p_folder_id UUID)
RETURNS TABLE (id UUID, name VARCHAR)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    SELECT "id", "name", "folder_id", 0 AS "depth"
    FROM "docbox_folders"
    WHERE "docbox_folders"."id" = p_folder_id

    UNION ALL

    SELECT
        "folder"."id",
        "folder"."name",
        "folder"."folder_id",
        "fh"."depth" + 1 as "depth"
    FROM "docbox_folders" AS "folder"
    INNER JOIN "folder_hierarchy" "fh" ON "folder"."id" = "fh"."folder_id"
)
SELECT "fh"."id", "fh"."name"
FROM "folder_hierarchy" "fh"
WHERE "fh"."id" <> p_folder_id
ORDER BY "fh"."depth" DESC
$$;

COMMENT ON FUNCTION resolve_folder_path(UUID)
IS 'Resolve the folder path that must be taken to reach a folder';

-- ================================================================
-- Resolve a collection of folder paths for `p_folder_ids` within
-- `p_document_box`
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folders_paths(p_document_box VARCHAR, p_folder_ids UUID[])
RETURNS TABLE (item_id UUID, path JSONB)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    -- Start from the parent folder to avoid including the folder itself
    SELECT
        "folder"."id" AS "item_id",
        "parent"."folder_id" AS "parent_folder_id",
        0 AS "depth",
        jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) AS "path"
    FROM "docbox_folders" "folder"
    JOIN docbox_folders "parent" ON "folder"."folder_id" = "parent"."id"
    WHERE "folder"."id" = ANY(p_folder_ids)
        AND "folder"."document_box" = p_document_box

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
-- Recursively resolve all the children (folder, file, and link)
-- ID's within the folder with the ID `p_folder_id`
-- ================================================================

CREATE OR REPLACE FUNCTION recursive_folder_children_ids(p_folder_id UUID)
RETURNS TABLE (id UUID)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE "folder_hierarchy" AS (
        SELECT "id", "folder_id"
        FROM "docbox_folders"
        WHERE "docbox_folders"."id" = p_folder_id

        UNION ALL

        SELECT
            "folder"."id",
            "folder"."folder_id"
        FROM "docbox_folders" AS "folder"
        INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
    )
    SELECT "folder_hierarchy"."id" FROM "folder_hierarchy"
$$;


-- ================================================================
-- Recursively count the number of children (folder, file, and link)
-- items within a folder with the ID `p_folder_Id`
-- ================================================================

CREATE OR REPLACE FUNCTION count_folder_children(p_folder_id UUID)
RETURNS TABLE (
    file_count BIGINT,
    link_count BIGINT,
    folder_count BIGINT
)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE "folder_hierarchy" AS (
        SELECT "id", "folder_id"
        FROM "docbox_folders"
        WHERE "id" = p_folder_id

        UNION ALL

        SELECT
            "folder"."id",
            "folder"."folder_id"
        FROM "docbox_folders" AS "folder"
        INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
    )
    SELECT
        COUNT(DISTINCT "file"."id") AS "file_count",
        COUNT(DISTINCT "link"."id") AS "link_count",
        COUNT(DISTINCT "folder"."id") AS "folder_count"
    FROM "folder_hierarchy"
    LEFT JOIN "docbox_files" AS "file" ON "file"."folder_id" = "folder_hierarchy"."id"
    LEFT JOIN "docbox_links" AS "link" ON "link"."folder_id" = "folder_hierarchy"."id"
    LEFT JOIN "docbox_folders" AS "folder" ON "folder"."folder_id" = "folder_hierarchy"."id"
$$;

-- ================================================================
-- Resolve a collection of folders with extra data using a
-- collection of input scope and folder ID pairs.
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folders_with_extra_mixed_scopes(
    p_input docbox_input_pair[]
)
RETURNS TABLE (
    folder docbox_folder,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path JSONB
)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE
        "input_folders" AS (
            SELECT folder_id, document_box
            FROM UNNEST(p_input) AS t(document_box, folder_id)
        ),
        "folder_hierarchy" AS (
            SELECT
                "folder"."id" AS "item_id",
                "parent"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                jsonb_build_array(jsonb_build_object('id', "parent"."id", 'name', "parent"."name")) AS "path"
            FROM "docbox_folders" "folder"
            JOIN "input_folders" "i" ON "folder"."id" = "i"."folder_id"
            JOIN "docbox_folders" "parent" ON "folder"."folder_id" = "parent"."id"
            WHERE "folder"."document_box" = "i"."document_box"

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
        mk_docbox_folder("folder") AS "folder",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path"
    FROM "docbox_folders" AS "folder"
    LEFT JOIN "docbox_users" AS "cu"
        ON "folder"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_folder" AS "ehl"
        ON "folder"."id" = "ehl"."folder_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN "folder_paths" "fp"
        ON "folder".id = "fp"."item_id" AND "fp".rn = 1
    JOIN "input_folders" "i"
        ON "folder"."id" = "i"."folder_id"
    WHERE "folder"."document_box" = "i"."document_box"
$$;


-- ================================================================
-- Resolve a collection of folders with extra data within
-- `p_document_box` using a collection of folder IDs
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folders_with_extra(
    p_document_box VARCHAR,
    p_folder_ids UUID[]
)
RETURNS TABLE (
    folder docbox_folder,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path JSONB
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_folder("folder") AS "folder",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path"
    FROM "docbox_folders" AS "folder"
    LEFT JOIN "docbox_users" AS "cu"
        ON "folder"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_folder" AS "ehl"
        ON "folder"."id" = "ehl"."folder_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN resolve_folders_paths(p_document_box, p_folder_ids) AS "fp"
        ON "folder".id = "fp"."item_id"
    WHERE "folder"."id" = ANY(p_folder_ids)
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolve a folder by ID within a single scope with extra
-- additional data like the folder path and the user details for
-- last modified and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folder_by_id_with_extra(
    p_document_box VARCHAR,
    p_folder_id UUID
)
RETURNS TABLE (
    folder docbox_folder,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_folder("folder") AS "folder",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_folders" AS "folder"
    LEFT JOIN "docbox_users" AS "cu"
        ON "folder"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_folder" AS "ehl"
        ON "folder"."id" = "ehl"."folder_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "folder"."id" = p_folder_id
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolve a collection of folders within a parent folder by ID
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folder_by_parent_with_extra(p_parent_id UUID)
RETURNS TABLE (
    folder docbox_folder,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_folder("folder") AS "folder",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_folders" AS "folder"
    LEFT JOIN "docbox_users" AS "cu"
        ON "folder"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_folder" AS "ehl"
        ON "folder"."id" = "ehl"."folder_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "folder"."folder_id" = p_parent_id
$$;


-- ================================================================
-- Resolve the root folder of a document box with additional data
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_root_folder_with_extra(p_document_box VARCHAR)
RETURNS TABLE (
    folder docbox_folder,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_folder("folder") AS "folder",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_folders" AS "folder"
    LEFT JOIN "docbox_users" AS "cu"
        ON "folder"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_folder" AS "ehl"
        ON "folder"."id" = "ehl"."folder_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "folder"."document_box" = p_document_box
        AND "folder"."folder_id" IS NULL
$$;
