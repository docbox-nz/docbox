-- Setup the tenants table
CREATE TABLE IF NOT EXISTS "docbox_tenants"
(
    "env"             VARCHAR NOT NULL,
    "id"              UUID    NOT NULL,
    "name"            VARCHAR NOT NULL,
    "db_name"         VARCHAR NOT NULL UNIQUE,
    "db_secret_name"  VARCHAR NOT NULL UNIQUE,
    "s3_name"         VARCHAR NOT NULL UNIQUE,
    "os_index_name"   VARCHAR NOT NULL UNIQUE,
    "event_queue_url" VARCHAR NULL,

    PRIMARY KEY ("env", "id")
);
