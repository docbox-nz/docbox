use docbox_database::models::edit_history::{
    CreateEditHistory, CreateEditHistoryType, EditHistory, EditHistoryMetadata, EditHistoryType,
};
use sqlx::types::Json;

use crate::common::{
    database::test_tenant_db, make_test_document_box, make_test_file, make_test_folder,
    make_test_link,
};

mod common;

/// Tests that an edit history item can be created
#[tokio::test]
async fn test_create_edit_history() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let link = make_test_link(&db, &root, "test", None).await;

    EditHistory::create(
        &db,
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: None,
            metadata: EditHistoryMetadata::LinkValue {
                previous_value: "a".to_string(),
                new_value: "b".to_string(),
            },
        },
    )
    .await
    .unwrap();

    let history = EditHistory::all_by_link(&db, link.id).await.unwrap();
    assert_eq!(history.len(), 1);

    let item = history.first().unwrap();
    assert!(item.file_id.is_none());
    assert!(item.folder_id.is_none());
    assert_eq!(item.link_id, Some(link.id));
    assert_eq!(item.user, None);
    assert_eq!(item.ty, EditHistoryType::LinkValue);
    assert_eq!(
        item.metadata,
        Json(EditHistoryMetadata::LinkValue {
            previous_value: "a".to_string(),
            new_value: "b".to_string(),
        })
    );
}

/// Tests that all edit history items can be found by a file
#[tokio::test]
async fn test_all_by_file_edit_history() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let file = make_test_file(&db, &root, "test", None).await;

    let history = EditHistory::all_by_file(&db, file.id).await.unwrap();
    assert!(history.is_empty());

    EditHistory::create(
        &db,
        CreateEditHistory {
            ty: CreateEditHistoryType::File(file.id),
            user_id: None,
            metadata: EditHistoryMetadata::Rename {
                original_name: "a".to_string(),
                new_name: "b".to_string(),
            },
        },
    )
    .await
    .unwrap();

    let history = EditHistory::all_by_file(&db, file.id).await.unwrap();
    assert_eq!(history.len(), 1);

    let item = history.first().unwrap();
    assert!(item.link_id.is_none());
    assert!(item.folder_id.is_none());
    assert_eq!(item.file_id, Some(file.id));
    assert_eq!(item.user, None);
    assert_eq!(item.ty, EditHistoryType::Rename);
    assert_eq!(
        item.metadata,
        Json(EditHistoryMetadata::Rename {
            original_name: "a".to_string(),
            new_name: "b".to_string(),
        })
    );
}

/// Tests that all edit history items can be found by a folder
#[tokio::test]
async fn test_all_by_folder_edit_history() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let folder = make_test_folder(&db, &root, "test", None).await;

    let history = EditHistory::all_by_folder(&db, folder.id).await.unwrap();
    assert!(history.is_empty());

    EditHistory::create(
        &db,
        CreateEditHistory {
            ty: CreateEditHistoryType::Folder(folder.id),
            user_id: None,
            metadata: EditHistoryMetadata::Rename {
                original_name: "a".to_string(),
                new_name: "b".to_string(),
            },
        },
    )
    .await
    .unwrap();

    let history = EditHistory::all_by_folder(&db, folder.id).await.unwrap();
    assert_eq!(history.len(), 1);

    let item = history.first().unwrap();
    assert!(item.file_id.is_none());
    assert!(item.link_id.is_none());
    assert_eq!(item.folder_id, Some(folder.id));
    assert_eq!(item.user, None);
    assert_eq!(item.ty, EditHistoryType::Rename);
    assert_eq!(
        item.metadata,
        Json(EditHistoryMetadata::Rename {
            original_name: "a".to_string(),
            new_name: "b".to_string(),
        })
    );
}

/// Tests that all edit history items can be found by a link
#[tokio::test]
async fn test_all_by_link_edit_history() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let link = make_test_link(&db, &root, "test", None).await;

    let history = EditHistory::all_by_link(&db, link.id).await.unwrap();
    assert!(history.is_empty());

    EditHistory::create(
        &db,
        CreateEditHistory {
            ty: CreateEditHistoryType::Link(link.id),
            user_id: None,
            metadata: EditHistoryMetadata::LinkValue {
                previous_value: "a".to_string(),
                new_value: "b".to_string(),
            },
        },
    )
    .await
    .unwrap();

    let history = EditHistory::all_by_link(&db, link.id).await.unwrap();
    assert_eq!(history.len(), 1);

    let item = history.first().unwrap();
    assert!(item.file_id.is_none());
    assert!(item.folder_id.is_none());
    assert_eq!(item.link_id, Some(link.id));
    assert_eq!(item.user, None);
    assert_eq!(item.ty, EditHistoryType::LinkValue);
    assert_eq!(
        item.metadata,
        Json(EditHistoryMetadata::LinkValue {
            previous_value: "a".to_string(),
            new_value: "b".to_string(),
        })
    );
}
