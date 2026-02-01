CREATE TYPE docbox_search_page_match AS (
    "page" INT8,
    "matched" TEXT,
    "content_match_rank" FLOAT8,
    "total_hits" INT8
);

COMMENT ON TYPE docbox_search_page_match
IS 'Search result match on a page of file content';

-- ================================================================

CREATE TYPE docbox_search_date_range AS (
    "start" TIMESTAMP WITH TIME ZONE,
    "end" TIMESTAMP WITH TIME ZONE
);

COMMENT ON TYPE docbox_search_date_range
IS 'Search filter for timestamp within a range, range may be open-ended if either side is NULL';

-- ================================================================

CREATE TYPE docbox_search_filters AS (
    -- Scope and folder
    "document_boxes" TEXT[],
    "folder_children" UUID[],

    -- Search fields
    "include_name" BOOLEAN,
    "include_content" BOOLEAN,

    -- Creation date and creator
    "created_at" docbox_search_date_range,
    "created_by" TEXT,

    -- Mime type filtering for files
    "mime" TEXT
);


COMMENT ON TYPE docbox_search_filters
IS 'Collection of common filter parameters shared amongst the various search functions';

-- ================================================================

CREATE TYPE docbox_search_item_type AS ENUM (
    'File',
    'Link',
    'Folder'
);

COMMENT ON TYPE docbox_search_item_type
IS 'Enumeration of possible item types that can be resolved when searching';

-- ================================================================

CREATE TYPE docbox_search_match AS (
    "item_type" docbox_search_item_type,
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

COMMENT ON TYPE docbox_search_match
IS 'Search result match for an item found as the result of a search';

-- ================================================================

CREATE TYPE docbox_search_match_ranked AS (
    "search_match" docbox_search_match,
    "rank" FLOAT8,
    "total_count" INT8
);

COMMENT ON TYPE docbox_search_match_ranked
IS 'Extension of docbox_search_match which contains a assigned rank for the match as well as a total_count for the total number of matches pre-pagination';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search_links(p_query_text TEXT, p_query_ts tsquery, p_filters docbox_search_filters)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'Link'::docbox_search_item_type AS "item_type",
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

COMMENT ON FUNCTION docbox_search_links(p_query_text TEXT, p_query_ts tsquery, p_filters docbox_search_filters)
IS 'Query search results within the links table';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search_folders(query_text TEXT, query_ts tsquery, p_filters docbox_search_filters)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'Folder'::docbox_search_item_type AS "item_type",
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

COMMENT ON FUNCTION docbox_search_folders(query_text TEXT, query_ts tsquery, p_filters docbox_search_filters)
IS 'Query search results within the folders table';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_file_has_matching_pages(p_file_id UUID, p_query_text TEXT, p_query_ts tsquery)
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

COMMENT ON FUNCTION docbox_file_has_matching_pages(p_file_id UUID, p_query_text TEXT, p_query_ts tsquery)
IS 'Helper to check the docbox_files_pages rows for a specific file to see if any page content contains the search query';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search_file_pages(p_file_id UUID, p_query_text TEXT, p_query_ts tsquery)
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

COMMENT ON FUNCTION docbox_search_file_pages(p_file_id UUID, p_query_text TEXT, p_query_ts tsquery)
IS 'Search for matches within the pages content for a file by ID';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search_file_pages_with_scope(
    p_document_box TEXT,
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
JOIN "docbox_files" "file" ON "file"."id" = p_file_id
JOIN "docbox_folders" "folder" ON "folder"."id" = "file"."folder_id"
WHERE "p"."file_id" = p_file_id
    AND "folder"."document_box" = p_document_box
    AND (
        -- Vector matching
        "p"."content_tsv" @@ p_query_ts
        -- Case insensitive exact matches
        OR "p"."content" ILIKE '%' || p_query_text || '%'
    )
ORDER BY "content_match_rank" DESC, "page" ASC
$$;


COMMENT ON FUNCTION docbox_search_file_pages_with_scope(
    p_document_box TEXT,
    p_file_id UUID,
    p_query_text TEXT,
    p_query_ts tsquery
)
IS 'Search for matches within the pages content for a file by ID, also ensures the file is within the p_document_box document box';


-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search_files(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
    p_max_pages INT8,
    p_pages_offset INT8
)
RETURNS SETOF docbox_search_match
LANGUAGE sql
STABLE
AS $$
SELECT
    'File'::docbox_search_item_type AS "item_type",
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
    FROM docbox_search_file_pages("file"."id", p_query_text, p_query_ts)
    LIMIT p_max_pages
    OFFSET p_pages_offset
) "pages" ON p_filters.include_content
WHERE "folder"."document_box" = ANY(p_filters.document_boxes)
    AND (p_filters.mime IS NULL OR "file"."mime" = p_filters.mime)
    AND ((p_filters).created_at.start IS NULL OR "file"."created_at" >= (p_filters).created_at.start)
    AND ((p_filters).created_at.end IS NULL OR "file"."created_at" <= (p_filters).created_at.end)
    AND (p_filters.created_by IS NULL OR "file"."created_by" = p_filters.created_by)
    AND (p_filters.folder_children IS NULL OR "file"."folder_id" = ANY(p_filters.folder_children))
    AND (
        (p_filters.include_name AND "file"."name_tsv" @@ p_query_ts)
        OR (p_filters.include_name AND "file"."name" ILIKE '%' || p_query_text || '%')
        OR (p_filters.include_content AND docbox_file_has_matching_pages("file"."id", p_query_text, p_query_ts))
    )
GROUP BY file.id, folder.document_box, file.name_tsv, file.name, file.created_at
$$;

COMMENT ON FUNCTION docbox_search_files(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
    p_max_pages INT8,
    p_pages_offset INT8
)
IS 'Search for matches within the pages content for a file by ID, also ensures the file is within the p_document_box document box';

-- ================================================================

CREATE OR REPLACE FUNCTION docbox_search(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
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
        SELECT * FROM docbox_search_links(p_query_text, p_query_ts, p_filters)
        UNION ALL
        SELECT * FROM docbox_search_folders(p_query_text, p_query_ts, p_filters)
        UNION ALL
        SELECT * FROM docbox_search_files(
            p_query_text,
            p_query_ts,
            p_filters,
            p_max_pages,
            p_pages_offset
        )
    ) "match"
    ORDER BY "rank" DESC, "created_at" DESC
$$;

COMMENT ON FUNCTION docbox_search(
    p_query_text TEXT,
    p_query_ts tsquery,
    p_filters docbox_search_filters,
    p_max_pages INT8,
    p_pages_offset INT8
)
IS 'Search for matches across files, links, and folders';
