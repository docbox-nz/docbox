-- Table for tracking migrations
CREATE TABLE IF NOT EXISTS "docbox_tenants_migrations"
(
    "env"            VARCHAR NOT NULL,
    "tenant_id"      UUID NOT NULL,
    "name"           VARCHAR NOT NULL,
    "applied_at"     TIMESTAMP WITH TIME ZONE NOT NULL,

    PRIMARY KEY ("env", "tenant_id", "name"),
    FOREIGN KEY ("env", "tenant_id")
        REFERENCES "docbox_tenants" ( "env", "id")
        ON DELETE CASCADE
);
