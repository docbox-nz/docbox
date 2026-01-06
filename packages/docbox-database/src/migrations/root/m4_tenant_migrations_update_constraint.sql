ALTER TABLE "docbox_tenants_migrations"
DROP CONSTRAINT IF EXISTS "docbox_tenants_migrations_env_tenant_id_fkey";

ALTER TABLE "docbox_tenants_migrations"
ADD CONSTRAINT "docbox_tenants_migrations_env_tenant_id_fkey"
FOREIGN KEY ("env", "tenant_id")
REFERENCES "docbox_tenants" ("env", "id")
ON DELETE CASCADE
ON UPDATE CASCADE;
