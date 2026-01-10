-- ================================================================
-- Resolve the path of a link by ID
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_link_path(p_link_id UUID)
RETURNS SETOF docbox_path_segment
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
SELECT ROW("fh"."id", "fh"."name")::docbox_path_segment
FROM "folder_hierarchy" "fh"
WHERE "fh"."id" <> p_link_id
ORDER BY "fh"."depth" DESC
$$;

-- ================================================================
-- Resolve a collection of link paths for `p_link_ids` within
-- `p_document_box`
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_links_paths(
    p_document_box VARCHAR,
    p_link_ids UUID[]
)
RETURNS TABLE (item_id UUID, path docbox_path_segment[])
LANGUAGE sql
STABLE
AS $$
WITH RECURSIVE "folder_hierarchy" AS (
    -- Start from the parent folder to avoid including the folder itself
    SELECT
        "link"."id" AS "item_id",
        "parent"."folder_id" AS "parent_folder_id",
        0 AS "depth",
        ARRAY[ROW("parent"."id", "parent"."name")::docbox_path_segment] AS "path"
    FROM "docbox_links" "link"
    JOIN docbox_folders "parent" ON "link"."folder_id" = "parent"."id"
    WHERE "link"."id" = ANY(p_link_ids)
        AND "parent"."document_box" = p_document_box

    UNION ALL

    SELECT
        "fh"."item_id",
        "parent"."folder_id",
        "fh"."depth" + 1,
        ARRAY[ROW("parent"."id", "parent"."name")::docbox_path_segment] || "fh"."path"
    FROM "folder_hierarchy" "fh"
    JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
)
SELECT DISTINCT ON ("item_id") "item_id", "path"
FROM "folder_hierarchy"
ORDER BY "item_id", "depth" DESC
$$;


-- ================================================================
-- Resolve links by IDs within a single scope with extra additional data like the
-- folder path and the user details for last modified and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_links_with_extra(
    p_document_box VARCHAR,
    p_link_ids UUID[]
)
RETURNS TABLE (
    link docbox_link,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path docbox_path_segment[]
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_link("link") AS "link",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path"
    FROM "docbox_links" AS "link"
    LEFT JOIN "docbox_users" AS "cu"
        ON "link"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "link"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_link" AS "ehl"
        ON "link"."id" = "ehl"."link_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN resolve_links_paths(p_document_box, p_link_ids) AS "fp"
        ON "link".id = "fp"."item_id"
    WHERE "link"."id" = ANY(p_link_ids)
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolve link by ID within a single scope with extra additional data like the
-- folder path and the user details for last modified and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_link_by_id_with_extra(
    p_document_box VARCHAR,
    p_link_id UUID
)
RETURNS TABLE (
    link docbox_link,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_link("link") AS "link",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_links" AS "link"
    LEFT JOIN "docbox_users" AS "cu"
        ON "link"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "link"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_link" AS "ehl"
        ON "link"."id" = "ehl"."link_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "link"."id" = p_link_id
        AND "folder"."document_box" = p_document_box
$$;

-- ================================================================
-- Resolves all links within a folder with extra additional data
-- like the creator and the last modification details
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_links_by_parent_folder_with_extra(
    p_parent_id UUID
)
RETURNS TABLE (
    link docbox_link,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE
)
LANGUAGE sql
STABLE
AS $$
    SELECT
        mk_docbox_link("link") AS "link",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at"
    FROM "docbox_links" AS "link"
    LEFT JOIN "docbox_users" AS "cu"
        ON "link"."created_by" = "cu"."id"
    INNER JOIN "docbox_folders" "folder"
        ON "link"."folder_id" = "folder"."id"
    LEFT JOIN "docbox_latest_edit_per_link" AS "ehl"
        ON "link"."id" = "ehl"."link_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    WHERE "link"."folder_id" = p_parent_id
$$;


-- ================================================================
-- Resolve links by scope and ID pairs with extra additional data
-- like the folder path and the user details for last modified
-- and creator
-- ================================================================

CREATE OR REPLACE FUNCTION resolve_links_with_extra_mixed_scopes(
    p_input docbox_input_pair[]
)
RETURNS TABLE (
    link docbox_link,
    created_by docbox_user,
    last_modified_by docbox_user,
    last_modified_at TIMESTAMP WITH TIME ZONE,
    full_path docbox_path_segment[],
    document_box VARCHAR
)
LANGUAGE sql
STABLE
AS $$
    WITH RECURSIVE
        "input_links" AS (
            SELECT document_box, link_id
            FROM UNNEST(p_input) AS t(document_box, link_id)
        ),
        "folder_hierarchy" AS (
            SELECT
                "link"."id" AS "item_id",
                "parent"."folder_id" AS "parent_folder_id",
                0 AS "depth",
                ARRAY[ROW("parent"."id", "parent"."name")::docbox_path_segment] AS "path"
            FROM "docbox_links" "link"
            JOIN "input_links" "i" ON "link"."id" = "i"."link_id"
            JOIN "docbox_folders" "parent" ON "link"."folder_id" = "parent"."id"
            WHERE "parent"."document_box" = "i"."document_box"

            UNION ALL

            SELECT
                "fh"."item_id",
                "parent"."folder_id",
                "fh"."depth" + 1,
                ARRAY[ROW("parent"."id", "parent"."name")::docbox_path_segment] || "fh"."path"
            FROM "folder_hierarchy" "fh"
            JOIN "docbox_folders" "parent" ON "fh"."parent_folder_id" = "parent"."id"
        ),
        "folder_paths" AS (
            SELECT DISTINCT ON ("item_id") "item_id", "path"
            FROM "folder_hierarchy"
            ORDER BY "item_id", "depth" DESC
        )
    SELECT
        mk_docbox_link("link") AS "link",
        mk_docbox_user("cu") AS "created_by",
        mk_docbox_user("mu") AS "last_modified_by",
        "ehl"."created_at" AS "last_modified_at",
        "fp"."path" AS "full_path",
        "folder"."document_box" AS "document_box"
    FROM "docbox_links" AS "link"
    INNER JOIN "docbox_folders" "folder"
        ON "link"."folder_id" = "folder"."id"
    INNER JOIN "input_links" "i"
        ON "link"."id" = "i"."link_id"
        AND "folder"."document_box" = "i"."document_box"
    LEFT JOIN "docbox_users" AS "cu"
        ON "link"."created_by" = "cu"."id"
    LEFT JOIN "docbox_latest_edit_per_link" AS "ehl"
        ON "link"."id" = "ehl"."link_id"
    LEFT JOIN "docbox_users" AS "mu"
        ON "ehl"."user_id" = "mu"."id"
    LEFT JOIN "folder_paths" "fp"
        ON "link".id = "fp"."item_id"
$$;
