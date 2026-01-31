

CREATE TYPE docbox_search_page_match AS (
    "page" INT8,
    "matched" TEXT,
    "content_match_rank" FLOAT8,
    "total_hits" INT8
);

CREATE TYPE docbox_search_date_range AS (
    "start" TIMESTAMP WITH TIME ZONE,
    "end" TIMESTAMP WITH TIME ZONE
);

CREATE TYPE docbox_search_filters AS (
    -- Scope and folder
    "document_boxes" TEXT[],
    "folder_children" UUID[],

    -- Search fields
    "include_name" BOOLEAN,
    "include_content" BOOLEAN,

    -- Creation date and creator
    "created_at" docbox_search_date_range,
    "created_by" TEXT
);

CREATE TYPE docbox_search_match AS (
    "item_type" VARCHAR,
    "item_id" UUID,
    "document_box" VARCHAR,
    "name_match_tsv" BOOLEAN,
    "name_match_tsv_rank" FLOAT8,
    "name_match" BOOLEAN,
    "content_match" BOOLEAN,
    "content_rank" FLOAT8,
    "total_hits" INT8,
    "page_matches" docbox_search_page_match[],
    "created_at" TIMESTAMP WITH TIME ZONE
);


CREATE TYPE docbox_search_match_ranked AS (
    "search_match" docbox_search_match,
    "rank" FLOAT8,
    "total_count" INT8
);

-- ================================================================

CREATE OR REPLACE FUNCTION resolve_link_search_candidates(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters
)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'Link' AS "item_type",
    "link"."id" AS "item_id",
    "folder"."document_box" AS "document_box",
    (p_filters.include_name AND "link"."name_tsv" @@ p_query_ts) AS "name_match_tsv",
    ts_rank("link"."name_tsv", p_query_ts) AS "name_match_tsv_rank",
    (p_filters.include_name AND "link"."name" ILIKE '%' || p_query_text || '%') AS "name_match",
    (p_filters.include_content AND "link"."value" ILIKE '%' || p_query_text || '%') AS "content_match",
    0::FLOAT8 as "content_rank",
    0::INT8 AS "total_hits",
    ARRAY[]::docbox_search_page_match[] AS "page_matches",
    "link"."created_at" AS "created_at"
FROM "docbox_links" "link"
LEFT JOIN "docbox_folders" "folder" ON "link"."folder_id" = "folder"."id"
WHERE "folder"."document_box" = ANY(p_filters.document_boxes)
    AND ((p_filters).created_at.start IS NULL OR "link"."created_at" >= (p_filters).created_at.start)
    AND ((p_filters).created_at.end IS NULL OR "link"."created_at" <= (p_filters).created_at.end)
    AND (p_filters.created_by IS NULL OR "link"."created_by" = p_filters.created_by)
    AND (p_filters.folder_children IS NULL OR "link"."folder_id" = ANY(p_filters.folder_children))
    AND (
        (p_filters.include_name AND "link"."name" ILIKE '%' || p_query_text || '%')
        OR (p_filters.include_name AND "link"."name_tsv" @@ p_query_ts)
        OR (p_filters.include_content AND "link"."value" ILIKE '%' || p_query_text || '%')
    )
$$;

-- ================================================================

CREATE OR REPLACE FUNCTION resolve_folder_search_candidates(
    query_text TEXT,
    query_ts tsquery,
    p_filters docbox_search_filters
)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'Folder' AS "item_type",
    "folder"."id" AS "item_id",
    "folder"."document_box" AS "document_box",
    (p_filters.include_name AND "folder"."name_tsv" @@ query_ts) AS "name_match_tsv",
    ts_rank("folder"."name_tsv", query_ts) AS "name_match_tsv_rank",
    (p_filters.include_name AND "folder"."name" ILIKE '%' || query_text || '%') AS "name_match",
    FALSE as "content_match",
    0::FLOAT8 as "content_rank",
    0::INT8 AS "total_hits",
    ARRAY[]::docbox_search_page_match[] AS "page_matches",
    "folder"."created_at" AS "created_at"
FROM "docbox_folders" "folder"
WHERE "folder"."document_box" = ANY(p_filters.document_boxes)
    AND ((p_filters).created_at.start IS NULL OR "folder"."created_at" >= (p_filters).created_at.start)
    AND ((p_filters).created_at.end IS NULL OR "folder"."created_at" <= (p_filters).created_at.end)
    AND (p_filters.created_by IS NULL OR "folder"."created_by" = p_filters.created_by)
    AND (p_filters.folder_children IS NULL OR "folder"."folder_id" = ANY(p_filters.folder_children))
    AND (
        (p_filters.include_name AND "folder"."name_tsv" @@ query_ts)
        OR (p_filters.include_name AND "folder"."name" ILIKE '%' || query_text || '%')
    )
$$;

-- ================================================================

CREATE OR REPLACE FUNCTION file_pages_has_search_match(
    p_file_id UUID,
    p_query_text TEXT,
    p_query_ts tsquery
)
RETURNS boolean
LANGUAGE sql
STABLE
AS $$
SELECT EXISTS (
    SELECT 1
    FROM "docbox_files_pages" "p"
    WHERE "p"."file_id" = p_file_id
        AND (
            -- Vector matching
            "p"."content_tsv" @@ p_query_ts
            -- Case insensitive exact matches
            OR "p"."content" ILIKE '%' || p_query_text || '%'
        )
)
$$;

COMMENT ON FUNCTION file_pages_has_search_match(
    p_file_id UUID,
    p_query_text TEXT,
    p_query_ts tsquery
)
IS 'Helper to check the docbox_files_pages rows for a specific file to see if any page content contains the search query';

-- ================================================================

CREATE OR REPLACE FUNCTION resolve_file_pages_search_candidates(
    p_file_id UUID,
    p_query_text TEXT,
    p_query_ts tsquery
)
RETURNS SETOF docbox_search_page_match
LANGUAGE sql
STABLE
AS $$
SELECT
    "p"."page" AS "page",
    ts_headline('english', "p"."content", p_query_ts, 'StartSel=<em>, StopSel=</em>') as "matched",
    (ts_rank("p"."content_tsv", p_query_ts)
       -- Boost result for ILIKE content matches
        + CASE WHEN "p"."content" ILIKE '%' || p_query_text || '%' THEN 1.0 ELSE 0 END
    ) AS "content_match_rank",
   COUNT(*) OVER () AS "total_hits"
FROM "docbox_files_pages" "p"
WHERE "p"."file_id" = p_file_id
    AND (
        -- Vector matching
        "p"."content_tsv" @@ p_query_ts
        -- Case insensitive exact matches
        OR "p"."content" ILIKE '%' || p_query_text || '%'
    )
ORDER BY "content_match_rank" DESC, "page" ASC
$$;

-- ================================================================

CREATE OR REPLACE FUNCTION resolve_file_search_candidates(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
    p_mime VARCHAR,
    p_max_pages INT8,
    p_pages_offset INT8
)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'File' AS "item_type",
    "file"."id" AS "item_id",
    "folder"."document_box" AS "document_box",
    (p_filters.include_name AND "file"."name_tsv" @@ p_query_ts) AS "name_match_tsv",
    ts_rank("file"."name_tsv", p_query_ts) AS "name_match_tsv_rank",
    (p_filters.include_name AND "file"."name" ILIKE '%' || p_query_text || '%') AS "name_match",
    (p_filters.include_content AND COUNT("pages"."page") > 0) AS "content_match",
    COALESCE(AVG("pages"."content_match_rank"), 0) as "content_rank",
    COALESCE(MAX("pages"."total_hits"), 0) AS "total_hits",
    ARRAY_AGG("pages"::docbox_search_page_match ORDER BY "pages"."content_match_rank" DESC, "pages"."page" ASC) AS "page_matches",
    "file"."created_at"
FROM "docbox_files" "file"
LEFT JOIN "docbox_folders" "folder"
    ON "file"."folder_id" = "folder"."id" AND "folder"."document_box" = ANY(p_filters.document_boxes)
LEFT JOIN LATERAL (
    SELECT *
    FROM resolve_file_pages_search_candidates("file"."id", p_query_text, p_query_ts)
    LIMIT p_max_pages
    OFFSET p_pages_offset
) "pages" ON p_filters.include_content
WHERE "folder"."document_box" = ANY(p_filters.document_boxes)
    AND (p_mime IS NULL OR "file"."mime" = p_mime)
    AND ((p_filters).created_at.start IS NULL OR "file"."created_at" >= (p_filters).created_at.start)
    AND ((p_filters).created_at.end IS NULL OR "file"."created_at" <= (p_filters).created_at.end)
    AND (p_filters.created_by IS NULL OR "file"."created_by" = p_filters.created_by)
    AND (p_filters.folder_children IS NULL OR "file"."folder_id" = ANY(p_filters.folder_children))
    AND (
        (p_filters.include_name AND "file"."name_tsv" @@ p_query_ts)
        OR (p_filters.include_name AND "file"."name" ILIKE '%' || p_query_text || '%')
        OR (p_filters.include_content AND file_pages_has_search_match("file"."id", p_query_text, p_query_ts))
    )
GROUP BY file.id, folder.document_box, file.name_tsv, file.name, file.created_at
$$;

-- ================================================================

CREATE OR REPLACE FUNCTION resolve_search_results(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
    p_mime VARCHAR,
    p_max_pages INT8,
    p_pages_offset INT8
)
RETURNS SETOF docbox_search_match_ranked
LANGUAGE sql
STABLE
AS $$
    SELECT
        "match"::docbox_search_match AS "search_match",
        ("name_match_tsv_rank"
        + "content_rank"
        + CASE WHEN "name_match" THEN 1.0 ELSE 0 END -- Boost result for ILIKE name matches
        + CASE WHEN "item_type" = 'Link' AND "content_match" THEN 1.0 ELSE 0 END -- Boost link content matches
        ) AS "rank",
        COUNT(*) OVER () as "total_count"
    FROM (
        SELECT * FROM resolve_link_search_candidates(p_query_text, p_query_ts, p_filters)
        UNION ALL
        SELECT * FROM resolve_folder_search_candidates(p_query_text, p_query_ts, p_filters)
        UNION ALL
        SELECT * FROM resolve_file_search_candidates(
            p_query_text,
            p_query_ts,
            p_filters,
            P_mime,
            p_max_pages,
            p_pages_offset
        )
    ) "match"
    ORDER BY "rank" DESC, "created_at" DESC
$$;
