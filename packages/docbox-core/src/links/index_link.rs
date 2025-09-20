use docbox_database::models::{document_box::DocumentBoxScopeRaw, link::Link};
use docbox_search::{
    TenantSearchIndex,
    models::{SearchIndexData, SearchIndexType},
};

use super::create_link::CreateLinkError;

pub async fn store_link_index(
    search: &TenantSearchIndex,
    link: &Link,
    scope: &DocumentBoxScopeRaw,
) -> Result<(), CreateLinkError> {
    let index = SearchIndexData {
        ty: SearchIndexType::Link,
        item_id: link.id,
        folder_id: link.folder_id,
        name: link.name.to_string(),
        mime: None,
        content: Some(link.value.clone()),
        pages: None,
        created_at: link.created_at,
        created_by: link.created_by.clone(),
        document_box: scope.clone(),
    };

    search
        .add_data(vec![index])
        .await
        .map_err(CreateLinkError::CreateIndex)?;

    Ok(())
}
