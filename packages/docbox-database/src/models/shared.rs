use sqlx::prelude::FromRow;

#[derive(Debug, FromRow)]
pub struct TotalSizeResult {
    pub total_size: i64,
}

#[derive(Debug, FromRow)]
pub struct CountResult {
    pub count: i64,
}
