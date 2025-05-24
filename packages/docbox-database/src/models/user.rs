use crate::{DbExecutor, DbResult};
use serde::Serialize;
use sqlx::prelude::FromRow;

pub type UserId = String;

#[derive(Debug, Clone, Serialize, FromRow)]
pub struct User {
    /// Unique ID of the user
    pub id: String,
    /// Last saved name for the user
    pub name: Option<String>,
    /// Last saved image ID for the user
    pub image_id: Option<String>,
}

impl User {
    /// Stores / updates the stored user data, returns back the user ID
    pub async fn store(
        db: impl DbExecutor<'_>,
        id: UserId,
        name: Option<String>,
        image_id: Option<String>,
    ) -> DbResult<User> {
        sqlx::query(
            r#"   
            INSERT INTO "docbox_users" ("id", "name", "image_id") 
            VALUES ($1, $2, $3)
            ON CONFLICT ("id") 
            DO UPDATE SET "name" = EXCLUDED."name", "image_id" = EXCLUDED."image_id"
        "#,
        )
        .bind(id.as_str())
        .bind(name.as_ref())
        .bind(image_id.as_ref())
        .execute(db)
        .await?;

        Ok(User { id, name, image_id })
    }

    #[allow(unused)]
    pub async fn find(db: impl DbExecutor<'_>, id: UserId) -> DbResult<Option<User>> {
        sqlx::query_as(r#"SELECT * FROM "docbox_users" WHERE "id" = $1"#)
            .bind(id)
            .fetch_optional(db)
            .await
    }
}
