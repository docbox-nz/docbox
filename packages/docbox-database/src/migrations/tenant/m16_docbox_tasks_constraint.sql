-- Delete any dangling tasks
DELETE FROM "docbox_tasks" "task"
WHERE NOT EXISTS (
      SELECT 1
      FROM "docbox_boxes" "box"
      WHERE "box"."scope" = "task"."document_box"
);

-- Add foreign key connecting tasks to document boxes
ALTER TABLE "docbox_tasks"
ADD CONSTRAINT "FK_docbox_tasks_document_box"
FOREIGN KEY ("document_box")
REFERENCES "docbox_boxes" ("scope")
ON DELETE CASCADE;
