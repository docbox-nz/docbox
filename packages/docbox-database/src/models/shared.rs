use ::sqlx::{
    Postgres,
    encode::{Encode, IsNull},
    postgres::{PgArgumentBuffer, PgHasArrayType, PgTypeInfo},
};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::types::PgRecordEncoder, prelude::FromRow};
use utoipa::ToSchema;
use uuid::Uuid;

use crate::models::{
    document_box::DocumentBoxScopeRaw,
    folder::{Folder, FolderId},
};

#[derive(Debug, FromRow)]
pub struct TotalSizeResult {
    pub total_size: i64,
}

#[derive(Debug, FromRow)]
pub struct CountResult {
    pub count: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, ToSchema, PartialEq, Eq, FromRow, sqlx::Type)]
#[sqlx(type_name = "docbox_path_segment")]
pub struct FolderPathSegment {
    #[schema(value_type = Uuid)]
    pub id: FolderId,
    pub name: String,
}

impl FolderPathSegment {
    pub fn new(id: FolderId, name: impl Into<String>) -> Self {
        Self {
            id,
            name: name.into(),
        }
    }
}

impl<'a> From<&'a Folder> for FolderPathSegment {
    fn from(value: &'a Folder) -> Self {
        Self::new(value.id, &value.name)
    }
}

pub struct DocboxInputPair<'a> {
    pub scope: &'a str,
    pub id: Uuid,
}

impl<'a> Encode<'_, Postgres> for DocboxInputPair<'a> {
    fn encode_by_ref(
        &self,
        buf: &mut PgArgumentBuffer,
    ) -> Result<IsNull, sqlx::error::BoxDynError> {
        let mut encoder = PgRecordEncoder::new(buf);
        encoder.encode(self.scope)?;
        encoder.encode(self.id)?;
        encoder.finish();
        Ok(IsNull::No)
    }

    fn size_hint(&self) -> ::std::primitive::usize {
        2usize * (4 + 4)
            + <&'a str as Encode<Postgres>>::size_hint(&self.scope)
            + <Uuid as Encode<Postgres>>::size_hint(&self.id)
    }
}

impl sqlx::Type<Postgres> for DocboxInputPair<'_> {
    fn type_info() -> PgTypeInfo {
        PgTypeInfo::with_name("docbox_input_pair")
    }
}

impl PgHasArrayType for DocboxInputPair<'_> {
    fn array_type_info() -> PgTypeInfo {
        PgTypeInfo::array_of("docbox_input_pair")
    }
}

impl<'a> DocboxInputPair<'a> {
    pub fn new(scope: &'a str, id: Uuid) -> Self {
        Self { scope, id }
    }
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WithFullPathScope<T> {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub data: T,
    pub full_path: Vec<FolderPathSegment>,
    pub document_box: DocumentBoxScopeRaw,
}

#[derive(Debug, Clone, FromRow, Serialize, Deserialize, ToSchema)]
pub struct WithFullPath<T> {
    #[serde(flatten)]
    #[sqlx(flatten)]
    pub data: T,
    pub full_path: Vec<FolderPathSegment>,
}
