use super::create_folder::CreateFolderError;
use crate::search::{
    models::{SearchIndexData, SearchIndexType},
    TenantSearchIndex,
};
use docbox_database::models::folder::{Folder, FolderId};

pub async fn store_folder_index(
    search: &TenantSearchIndex,
    folder: &Folder,
    folder_id: FolderId,
) -> Result<(), CreateFolderError> {
    // Add folder to search index
    search
        .add_data(SearchIndexData {
            ty: SearchIndexType::Folder,
            item_id: folder.id,
            folder_id,
            name: folder.name.to_string(),
            mime: None,
            content: None,
            pages: None,
            created_at: folder.created_at,
            created_by: folder.created_by.clone(),
            document_box: folder.document_box.clone(),
        })
        .await
        .map_err(CreateFolderError::CreateIndex)?;

    Ok(())
}
