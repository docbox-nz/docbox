use docbox_database::models::{document_box::DocumentBoxScopeRaw, file::CreateFile};
use docbox_processing::ProcessingIndexMetadata;
use docbox_search::{
    TenantSearchIndex,
    models::{SearchIndexData, SearchIndexType},
};

use super::upload_file::UploadFileError;

pub async fn store_file_index(
    search: &TenantSearchIndex,
    file: &CreateFile,
    document_box: &DocumentBoxScopeRaw,
    index_metadata: Option<ProcessingIndexMetadata>,
) -> Result<(), UploadFileError> {
    let index = SearchIndexData {
        ty: SearchIndexType::File,
        item_id: file.id,
        folder_id: file.folder_id,
        name: file.name.to_string(),
        mime: Some(file.mime.clone()),
        content: None,
        created_at: file.created_at,
        created_by: file.created_by.clone(),
        document_box: document_box.clone(),
        pages: index_metadata.and_then(|value| value.pages),
    };

    search
        .add_data(vec![index])
        .await
        .map_err(UploadFileError::CreateIndex)?;

    Ok(())
}
