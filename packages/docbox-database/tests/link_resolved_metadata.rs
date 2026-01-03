use chrono::{Days, Utc};
use docbox_database::models::link_resolved_metadata::{
    CreateLinkResolvedMetadata, LinkResolvedMetadata, StoredResolvedWebsiteMetadata,
};

use crate::common::database::test_tenant_db;

mod common;

/// Tests that resolved link metadata can be created and retrieved
#[tokio::test]
async fn test_create_resolved_link_metadata() {
    let (db, _db_container) = test_tenant_db().await;
    LinkResolvedMetadata::create(
        &db,
        CreateLinkResolvedMetadata {
            url: "http://test.com".to_string(),
            metadata: StoredResolvedWebsiteMetadata {
                best_favicon: None,
                og_description: None,
                og_image: None,
                og_title: None,
                title: None,
            },
            expires_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = LinkResolvedMetadata::query(&db, "http://test.com")
        .await
        .unwrap()
        .expect("should have resolved metadata");
    assert_eq!(result.url.as_str(), "http://test.com");
    assert_eq!(
        result.metadata,
        StoredResolvedWebsiteMetadata {
            best_favicon: None,
            og_description: None,
            og_image: None,
            og_title: None,
            title: None,
        }
    );
}

/// Tests that a duplicate resolved metadata creation should simply update the
/// existing metadata instead of replacing it
#[tokio::test]
async fn test_update_resolved_link_metadata() {
    let (db, _db_container) = test_tenant_db().await;
    LinkResolvedMetadata::create(
        &db,
        CreateLinkResolvedMetadata {
            url: "http://test.com".to_string(),
            metadata: StoredResolvedWebsiteMetadata {
                best_favicon: None,
                og_description: None,
                og_image: None,
                og_title: None,
                title: None,
            },
            expires_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = LinkResolvedMetadata::query(&db, "http://test.com")
        .await
        .unwrap()
        .expect("should have resolved metadata");
    assert_eq!(result.url.as_str(), "http://test.com");
    assert_eq!(
        result.metadata,
        StoredResolvedWebsiteMetadata {
            best_favicon: None,
            og_description: None,
            og_image: None,
            og_title: None,
            title: None,
        }
    );

    LinkResolvedMetadata::create(
        &db,
        CreateLinkResolvedMetadata {
            url: "http://test.com".to_string(),
            metadata: StoredResolvedWebsiteMetadata {
                best_favicon: None,
                og_description: None,
                og_image: None,
                og_title: None,
                title: None,
            },
            expires_at: Utc::now(),
        },
    )
    .await
    .unwrap();

    let result = LinkResolvedMetadata::query(&db, "http://test.com")
        .await
        .unwrap()
        .expect("should have resolved metadata");
    assert_eq!(result.url.as_str(), "http://test.com");
    assert_eq!(
        result.metadata,
        StoredResolvedWebsiteMetadata {
            best_favicon: None,
            og_description: None,
            og_image: None,
            og_title: None,
            title: None,
        }
    );
}

/// Tests that expired link metadata is correctly deleted
#[tokio::test]
async fn test_delete_resolved_link_metadata() {
    let (db, _db_container) = test_tenant_db().await;
    LinkResolvedMetadata::create(
        &db,
        CreateLinkResolvedMetadata {
            url: "http://test.com".to_string(),
            metadata: StoredResolvedWebsiteMetadata {
                best_favicon: None,
                og_description: None,
                og_image: None,
                og_title: None,
                title: None,
            },
            expires_at: Utc::now().checked_sub_days(Days::new(1)).unwrap(),
        },
    )
    .await
    .unwrap();

    LinkResolvedMetadata::create(
        &db,
        CreateLinkResolvedMetadata {
            url: "http://test2.com".to_string(),
            metadata: StoredResolvedWebsiteMetadata {
                best_favicon: None,
                og_description: None,
                og_image: None,
                og_title: None,
                title: None,
            },
            expires_at: Utc::now().checked_add_days(Days::new(1)).unwrap(),
        },
    )
    .await
    .unwrap();

    LinkResolvedMetadata::delete_expired(&db, Utc::now())
        .await
        .unwrap();

    let result = LinkResolvedMetadata::query(&db, "http://test.com")
        .await
        .unwrap();
    assert!(result.is_none());

    let result = LinkResolvedMetadata::query(&db, "http://test2.com")
        .await
        .unwrap()
        .expect("should have resolved metadata");
    assert_eq!(result.url.as_str(), "http://test2.com");
    assert_eq!(
        result.metadata,
        StoredResolvedWebsiteMetadata {
            best_favicon: None,
            og_description: None,
            og_image: None,
            og_title: None,
            title: None,
        }
    );
}
