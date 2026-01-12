use chrono::Utc;
use docbox_database::models::generated_file::{
    CreateGeneratedFile, GeneratedFile, GeneratedFileType,
};
use uuid::Uuid;

use crate::common::{database::test_tenant_db, make_test_document_box, make_test_file};

mod common;

/// Tests that a generated file can be created
#[tokio::test]
async fn test_generated_file_create() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;
    let file = make_test_file(&db, &root, "test", None).await;

    let generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/pdf".to_string(),
            ty: GeneratedFileType::Pdf,
            hash: "aabbcc".to_string(),
            file_key: "test/key".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = GeneratedFile::find(&db, &document_box.scope, file.id, GeneratedFileType::Pdf)
        .await
        .unwrap();

    assert_eq!(result, Some(generated_file));
}

/// Tests that multiple generated files of the same type cannot be added
///
/// TODO: This is not currently enforced on the DB level, do we want to enforce this?
#[tokio::test]
async fn test_generated_file_create_duplicate_error() {
    // let (db, _db_container) = test_tenant_db().await;
    // let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    // let file = make_test_file(&db, &root, "test", None).await;

    // let _generated_file_1 = GeneratedFile::create(
    //     &db,
    //     CreateGeneratedFile {
    //         id: Uuid::new_v4(),
    //         file_id: file.id,
    //         mime: "application/pdf".to_string(),
    //         ty: GeneratedFileType::Pdf,
    //         hash: "aabbcc".to_string(),
    //         file_key: "test/key".to_string(),
    //         created_at: Utc::now(),
    //     },
    // )
    // .await
    // .unwrap();

    // let error = GeneratedFile::create(
    //     &db,
    //     CreateGeneratedFile {
    //         id: Uuid::new_v4(),
    //         file_id: file.id,
    //         mime: "application/pdf".to_string(),
    //         ty: GeneratedFileType::Pdf,
    //         hash: "aabbcc".to_string(),
    //         file_key: "test/key".to_string(),
    //         created_at: Utc::now(),
    //     },
    // )
    // .await
    // .unwrap_err();

    // assert!(error.is_duplicate_record());
}

/// Tests that a generated file can be deleted
#[tokio::test]
async fn test_delete_generated_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;
    let file = make_test_file(&db, &root, "test", None).await;

    let generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/pdf".to_string(),
            ty: GeneratedFileType::Pdf,
            hash: "aabbcc".to_string(),
            file_key: "test/key".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let other_generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/json".to_string(),
            ty: GeneratedFileType::Metadata,
            hash: "aabbcc".to_string(),
            file_key: "test/key2".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    // Should delete row
    let result = generated_file.clone().delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 1);

    // Generated file should be none
    let result = GeneratedFile::find(&db, &document_box.scope, file.id, GeneratedFileType::Pdf)
        .await
        .unwrap();
    assert_eq!(result, None);

    // Other generated file should still exist
    let result = GeneratedFile::find(
        &db,
        &document_box.scope,
        file.id,
        GeneratedFileType::Metadata,
    )
    .await
    .unwrap();
    assert_eq!(result, Some(other_generated_file.clone()));

    // Delete shouldn't affect any rows
    let result = generated_file.clone().delete(&db).await.unwrap();
    assert_eq!(result.rows_affected(), 0);

    // Shouldn't be able to delete the file itself while a generated file exists
    let error = file.delete(&db).await.unwrap_err();
    assert_eq!(
        error.into_database_error().unwrap().code().unwrap(),
        // foreign key constraint restrict violation
        "23001"
    );

    other_generated_file.delete(&db).await.unwrap();

    // Should be able to delete file after the final generated file is gone
    file.delete(&db).await.unwrap();
}

/// Tests that all the generated files for a file can be found
#[tokio::test]
async fn test_find_all_generated_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (_document_box, root) = make_test_document_box(&db, "test", None).await;
    let file = make_test_file(&db, &root, "test", None).await;

    let generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/pdf".to_string(),
            ty: GeneratedFileType::Pdf,
            hash: "aabbcc".to_string(),
            file_key: "test/key".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let other_generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/json".to_string(),
            ty: GeneratedFileType::Metadata,
            hash: "aabbcc".to_string(),
            file_key: "test/key2".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = GeneratedFile::find_all(&db, file.id).await.unwrap();
    assert_eq!(result.len(), 2);
    assert!(result.iter().any(|value| value.eq(&generated_file)));
    assert!(result.iter().any(|value| value.eq(&other_generated_file)));
}

/// Tests that a generated file can be found by type for a file
#[tokio::test]
async fn test_find_generated_file() {
    let (db, _db_container) = test_tenant_db().await;
    let (document_box, root) = make_test_document_box(&db, "test", None).await;
    let file = make_test_file(&db, &root, "test", None).await;

    let generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/pdf".to_string(),
            ty: GeneratedFileType::Pdf,
            hash: "aabbcc".to_string(),
            file_key: "test/key".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let other_generated_file = GeneratedFile::create(
        &db,
        CreateGeneratedFile {
            id: Uuid::new_v4(),
            file_id: file.id,
            mime: "application/json".to_string(),
            ty: GeneratedFileType::Metadata,
            hash: "aabbcc".to_string(),
            file_key: "test/key2".to_string(),
            created_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = GeneratedFile::find(&db, &document_box.scope, file.id, GeneratedFileType::Pdf)
        .await
        .unwrap();

    assert_eq!(result, Some(generated_file));

    let result = GeneratedFile::find(
        &db,
        &document_box.scope,
        file.id,
        GeneratedFileType::Metadata,
    )
    .await
    .unwrap();

    assert_eq!(result, Some(other_generated_file));
}
