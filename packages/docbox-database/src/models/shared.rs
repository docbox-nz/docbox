use sqlx::prelude::FromRow;

#[derive(Debug, FromRow)]
pub struct TotalSizeResult {
    pub total_size: i64,
}

#[derive(Debug, FromRow)]
pub struct CountResult {
    pub count: i64,
}

#[derive(sqlx::Type)]
#[sqlx(type_name = "folder_input")]
pub struct FolderInput {
    pub document_box: String,
    pub folder_id: uuid::Uuid,
}
