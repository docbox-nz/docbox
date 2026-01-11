use docbox_database::{
    DbPool,
    models::{
        document_box::DocumentBox,
        folder::{CreateFolder, Folder},
        user::User,
    },
};
use uuid::Uuid;

pub mod database;

/// Make a test document box (and root folder) for test case with an
/// optional created by user
#[allow(unused)]
pub async fn make_test_document_box(
    db: &DbPool,
    scope: impl Into<String>,
    created_by: Option<String>,
) -> (DocumentBox, Folder) {
    let document_box = DocumentBox::create(db, scope.into()).await.unwrap();
    let root = Folder::create(
        db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: document_box.scope.clone(),
            folder_id: None,
            created_by,
        },
    )
    .await
    .unwrap();

    (document_box, root)
}

/// Make a test folder
#[allow(unused)]
pub async fn make_test_folder(
    db: &DbPool,
    parent: &Folder,
    name: impl Into<String>,
    created_by: Option<String>,
) -> Folder {
    Folder::create(
        db,
        CreateFolder {
            name: name.into(),
            document_box: parent.document_box.clone(),
            folder_id: Some(parent.id),
            created_by,
        },
    )
    .await
    .unwrap()
}

/// Make a test user
#[allow(unused)]
pub async fn make_test_user(db: &DbPool, name: impl Into<String>) -> User {
    User::store(
        db,
        Uuid::new_v4().to_string(),
        Some(name.into()),
        Some("image.png".to_string()),
    )
    .await
    .unwrap()
}
