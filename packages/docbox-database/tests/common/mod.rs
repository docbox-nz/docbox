use docbox_database::{
    DbPool,
    models::{
        document_box::DocumentBox,
        file::{CreateFile, File},
        folder::{CreateFolder, Folder},
        link::{CreateLink, Link},
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

/// Make a test link
#[allow(unused)]
pub async fn make_test_link(
    db: &DbPool,
    parent: &Folder,
    name: impl Into<String>,
    created_by: Option<String>,
) -> Link {
    Link::create(
        db,
        CreateLink {
            name: name.into(),
            // Random UUID value to ensure the value is captured from the DB properly
            // for testing asserts
            value: Uuid::new_v4().to_string(),
            folder_id: parent.id,
            created_by,
        },
    )
    .await
    .unwrap()
}

/// Make a test link
#[allow(unused)]
pub async fn make_test_file(
    db: &DbPool,
    parent: &Folder,
    name: impl Into<String>,
    created_by: Option<String>,
) -> File {
    File::create(
        db,
        CreateFile {
            id: Uuid::new_v4(),
            name: name.into(),
            folder_id: parent.id,
            created_by,
            ..Default::default()
        },
    )
    .await
    .unwrap()
}

/// Make a test link
#[allow(unused)]
pub async fn make_test_file_type(
    db: &DbPool,
    parent: &Folder,
    name: impl Into<String>,
    mime: impl Into<String>,
    created_by: Option<String>,
) -> File {
    File::create(
        db,
        CreateFile {
            id: Uuid::new_v4(),
            name: name.into(),
            folder_id: parent.id,
            created_by,
            mime: mime.into(),
            ..Default::default()
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
