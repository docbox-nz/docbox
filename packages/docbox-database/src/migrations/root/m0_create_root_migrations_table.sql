-- Table for tracking migrations
CREATE TABLE IF NOT EXISTS "docbox_root_migrations"
(
    "name"           VARCHAR NOT NULL,
    "applied_at"     TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY ("name")
);
