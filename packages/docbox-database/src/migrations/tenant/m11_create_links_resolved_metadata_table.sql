CREATE TABLE "docbox_links_resolved_metadata"
(
    "url"        VARCHAR NOT NULL PRIMARY KEY,
    "metadata"   JSONB NOT NULL,
    "expires_at" TIMESTAMP WITH TIME ZONE NOT NULL
);
