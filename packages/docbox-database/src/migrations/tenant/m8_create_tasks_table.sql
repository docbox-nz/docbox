CREATE TABLE "docbox_tasks"
(
    "id"           UUID                     NOT NULL
        PRIMARY KEY,
    "document_box" VARCHAR                  NOT NULL,
    "status"       TEXT                     NOT NULL,
    "output_data"  JSONB,
    "created_at"   TIMESTAMP WITH TIME ZONE NOT NULL,
    "completed_at" TIMESTAMP WITH TIME ZONE
);
