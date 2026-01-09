-- ==== ==== ==== ====
-- Resolve the path of a folder by ID
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION resolve_folder_path(
    p_folder_id uuid
)
RETURNS TABLE (
    id uuid,
    name text
)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    SELECT "id", "name", "folder_id", 0 AS "depth"
    FROM "docbox_folders"
    WHERE "docbox_folders"."id" = p_folder_id
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
CYCLE "id" SET "looped" USING "traversal_path"
SELECT "folder_hierarchy"."id", "folder_hierarchy"."name"
FROM "folder_hierarchy"
WHERE "folder_hierarchy"."id" <> p_folder_id
ORDER BY "folder_hierarchy"."depth" DESC
$$;

-- ==== ==== ==== ====
-- Resolve the path of a link by ID
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION resolve_link_path(
    p_link_id uuid
)
RETURNS TABLE (
    id uuid,
    name text
)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    SELECT "id", "name", "folder_id", 0 AS "depth"
    FROM "docbox_links"
    WHERE "docbox_links"."id" = p_link_id
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
CYCLE "id" SET "looped" USING "traversal_path"
SELECT "folder_hierarchy"."id", "folder_hierarchy"."name"
FROM "folder_hierarchy"
WHERE "folder_hierarchy"."id" <> p_link_id
ORDER BY "folder_hierarchy"."depth" DESC
$$;

-- ==== ==== ==== ====
-- Resolve the path of a file by ID
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION resolve_file_path(
    p_file_id uuid
)
RETURNS TABLE (
    id uuid,
    name text
)
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
CYCLE "id" SET "looped" USING "traversal_path"
SELECT "folder_hierarchy"."id", "folder_hierarchy"."name"
FROM "folder_hierarchy"
WHERE "folder_hierarchy"."id" <> p_file_id
ORDER BY "folder_hierarchy"."depth" DESC
$$;

-- ==== ==== ==== ====
-- Resolve a collection of folder paths for `p_folder_ids` withing `p_document_box`
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION resolve_folders_paths(
    p_folder_ids uuid[],
    p_document_box varchar
)
RETURNS TABLE (
    item_id uuid,
    path jsonb
)
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    -- Base collection (Start from the parent folder to avoid including the folder itself)
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

    -- Prepend parent folders
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

-- ==== ==== ==== ====
-- Recursively resolve the IDs of all child folders within a folder
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION recursive_folder_children_ids(
    p_folder_id uuid
)
RETURNS TABLE (
    id uuid
)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE "folder_hierarchy" AS (
        SELECT "id", "folder_id"
        FROM "docbox_folders"
        WHERE "docbox_folders"."id" = p_folder_id
        UNION ALL (
            SELECT
                "folder"."id",
                "folder"."folder_id"
            FROM "docbox_folders" AS "folder"
            INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
        )
    )
    CYCLE "id" SET "looped" USING "traversal_path"
    SELECT "folder_hierarchy"."id" FROM "folder_hierarchy"
$$;

-- ==== ==== ==== ====
-- Recursively count the number of items within a folder and sub folders
-- ==== ==== ==== ====

CREATE OR REPLACE FUNCTION count_folder_children(
    p_folder_id uuid
)
RETURNS TABLE (
    file_count BIGINT,
    link_count BIGINT,
    folder_count BIGINT
)
LANGUAGE sql
STABLE
AS $$
    -- Recursively collect all child folders
    WITH RECURSIVE "folder_hierarchy" AS (
        SELECT "id", "folder_id"
        FROM "docbox_folders"
        WHERE "docbox_folders"."id" = p_folder_id
        UNION ALL (
            SELECT
                "folder"."id",
                "folder"."folder_id"
            FROM "docbox_folders" AS "folder"
            INNER JOIN "folder_hierarchy" ON "folder"."folder_id" = "folder_hierarchy"."id"
        )
    )
    CYCLE "id" SET "looped" USING "traversal_path"
    SELECT
        -- Get counts of child tables
        COUNT(DISTINCT "file"."id") AS "file_count",
        COUNT(DISTINCT "link"."id") AS "link_count",
        COUNT(DISTINCT "folder"."id") AS "folder_count"
    FROM "folder_hierarchy"
    -- Join on collections of files, links and folders
    LEFT JOIN "docbox_files" AS "file" ON "file"."folder_id" = "folder_hierarchy"."id"
    LEFT JOIN "docbox_links" AS "link" ON "link"."folder_id" = "folder_hierarchy"."id"
    LEFT JOIN "docbox_folders" AS "folder" ON "folder"."folder_id" = "folder_hierarchy"."id"
$$;
