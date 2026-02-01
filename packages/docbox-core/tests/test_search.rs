use std::time::Duration;

use docbox_core::files::index_file::create_file_index;
use docbox_database::{
    DbPool,
    migrations::apply_tenant_migrations,
    models::{
        document_box::DocumentBox,
        file::{CreateFile, File},
        folder::{CreateFolder, Folder},
        tenant::{CreateTenant, Tenant},
    },
};
use docbox_processing::ProcessingIndexMetadata;
use docbox_search::{
    DatabaseSearchIndex, TenantSearchIndex,
    models::{DocumentPage, FileSearchRequest, SearchRequest},
};
use itertools::Itertools;
use testcontainers::{ContainerAsync, GenericImage};
use testcontainers_modules::postgres::Postgres;
use tokio::time::Instant;
use uuid::{Uuid, uuid};

use crate::common::{
    database::{test_database, test_database_container, test_root_db},
    typesense::{test_search_factory, test_typesense_container},
};
use fake::{Fake, faker::lorem::zh_tw::Words};
use futures::{StreamExt, stream::FuturesUnordered};

mod common;

const SEARCH_TEXT_CONTENT: &str = include_str!("./samples/search_text_content_target.txt");
const SEARCH_TEXT_PHRASE: &str = "Within this content there is a very specific sentence we are searching for, this is that sentence.";
const SEED_FILE_ID: Uuid = uuid!("278397ec-7f78-4a90-a4be-f4d7df6841a7");

const SEARCH_TEXT_CONTENT_2: &str = include_str!("./samples/search_text_content_target_2.txt");
const SEARCH_TEXT_PHRASE_2: &str = "Here is another piece of content I would like to match";
const SEED_FILE_ID_2: Uuid = uuid!("ceaaa68f-df24-4038-b424-db7c0fc6d98a");

async fn seed_index_data(
    db: &DbPool,
    search: &TenantSearchIndex,
    scope: &str,
    samples: usize,
) -> (DocumentBox, Folder) {
    let mut files = Vec::new();
    let mut search_data = Vec::new();

    let scope = scope.to_string();
    let document_box = DocumentBox::create(db, scope.clone()).await.unwrap();
    let root = Folder::create(
        db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope.clone(),
            folder_id: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    for _ in 0..samples {
        let id = Uuid::new_v4();
        let create_file = CreateFile {
            id,
            name: id.to_string(),
            folder_id: root.id,
            ..Default::default()
        };

        let mut pages = Vec::with_capacity(50);

        for i in 0..2 {
            pages.push(DocumentPage {
                page: i,
                content: Words(50..5000).fake::<Vec<String>>().join(" "),
            });
        }

        let index_metadata = ProcessingIndexMetadata { pages: Some(pages) };

        let data = create_file_index(&create_file, &scope, Some(index_metadata));
        files.push(create_file);
        search_data.push(data);
    }

    println!("generated data for seeding");

    // Insert search data in batches of 5k
    let chunks = search_data.into_iter().chunks(1000);

    let chunks = chunks.into_iter();
    let mut stream = chunks
        .map(|chunk| {
            let chunk = chunk.collect::<Vec<_>>();

            async move {
                search.add_data(chunk).await.unwrap();
                println!("stored chunk of search data");
            }
        })
        .collect::<FuturesUnordered<_>>();

    while let Some(_item) = stream.next().await {}

    println!("stored search data");

    // Create files in batches of 100
    for create in files.chunks(100) {
        let mut futures = create
            .iter()
            .map(|file| File::create(db, file.clone()))
            .collect::<FuturesUnordered<_>>();

        while let Some(result) = futures.next().await {
            result.unwrap();
        }
    }

    (document_box, root)
}

#[allow(unused)]
struct TestTenant {
    tenant: Tenant,
    //
    root_db: DbPool,
    root_db_container: ContainerAsync<Postgres>,
    //
    tenant_db: DbPool,
    tenant_db_container: ContainerAsync<Postgres>,
}

async fn create_test_tenant() -> TestTenant {
    // Initialize root database
    let (root_db, root_db_container) = test_root_db().await;

    // Initialize test tenant
    let tenant_id = Uuid::new_v4();
    let tenant = Tenant::create(
        &root_db,
        CreateTenant {
            id: tenant_id,
            name: "test".to_string(),
            db_name: "test".to_string(),
            db_secret_name: Some("test".to_string()),
            db_iam_user_name: None,
            s3_name: "test".to_string(),
            os_index_name: "test".to_string(),
            env: "Development".to_string(),
            event_queue_url: None,
        },
    )
    .await
    .unwrap();

    // Create tenant database
    let tenant_db_container = test_database_container().await;
    let tenant_db = test_database(&tenant_db_container).await;

    // Apply tenant migrations properly
    {
        let mut root_trans = root_db.begin().await.unwrap();
        let mut trans = tenant_db.begin().await.unwrap();
        apply_tenant_migrations(&mut root_trans, &mut trans, &tenant, None)
            .await
            .unwrap();
        trans.commit().await.unwrap();
        root_trans.commit().await.unwrap();
    }

    TestTenant {
        tenant,
        root_db,
        root_db_container,
        tenant_db,
        tenant_db_container,
    }
}

async fn create_search_index_typesense(
    test_tenant: &TestTenant,
) -> (TenantSearchIndex, ContainerAsync<GenericImage>) {
    let container = test_typesense_container().await;
    let search = test_search_factory(&container).await;
    let index = search.create_search_index(&test_tenant.tenant);

    index.create_index().await.unwrap();

    (index, container)
}

async fn create_search_index_database(test_tenant: &TestTenant) -> TenantSearchIndex {
    // Create search index
    let search = TenantSearchIndex::Database(DatabaseSearchIndex::from_pool(
        test_tenant.tenant_db.clone(),
    ));
    search.create_index().await.unwrap();

    // Apply search migrations
    {
        let mut root_trans = test_tenant.root_db.begin().await.unwrap();
        let mut trans = test_tenant.tenant_db.begin().await.unwrap();

        search
            .apply_migrations(&test_tenant.tenant, &mut root_trans, &mut trans, None)
            .await
            .unwrap();

        trans.commit().await.unwrap();
        root_trans.commit().await.unwrap();
    }

    search
}

#[tokio::test]
async fn test_search() {
    let test_tenant = create_test_tenant().await;
    let search = create_search_index_database(&test_tenant).await;

    let db = &test_tenant.tenant_db;

    let scope = "test".to_string();
    let _document_box = DocumentBox::create(db, scope.clone()).await.unwrap();
    let root = Folder::create(
        db,
        CreateFolder {
            name: "Root".to_string(),
            document_box: scope.clone(),
            folder_id: None,
            ..Default::default()
        },
    )
    .await
    .unwrap();

    // Seed our special search content
    let create_file = CreateFile {
        id: SEED_FILE_ID,
        name: SEED_FILE_ID.to_string(),
        folder_id: root.id,
        ..Default::default()
    };

    let data = create_file_index(
        &create_file,
        &scope,
        Some(ProcessingIndexMetadata {
            pages: Some(vec![DocumentPage {
                page: 0,
                content: SEARCH_TEXT_CONTENT.to_string(),
            }]),
        }),
    );
    File::create(db, create_file).await.unwrap();
    search.add_data(vec![data]).await.unwrap();

    let results = search
        .search_index(
            &["test".to_string()],
            SearchRequest {
                query: Some(SEARCH_TEXT_PHRASE.to_string()),
                include_content: true,
                ..Default::default()
            },
            None,
        )
        .await
        .unwrap();

    assert_eq!(results.total_hits, 1);
    let results = search
        .search_index_file(
            &"test".to_string(),
            SEED_FILE_ID,
            FileSearchRequest {
                query: Some(SEARCH_TEXT_PHRASE.to_string()),
                ..Default::default()
            },
        )
        .await
        .unwrap();

    assert_eq!(results.total_hits, 1);
    // dbg!(&results);
}

#[tokio::test]
#[ignore = "Benchmarking test for search performance"]
async fn test_search_database() {
    let test_tenant = create_test_tenant().await;
    let search = create_search_index_database(&test_tenant).await;

    println!("finished initialize");

    let (document_box, root) = seed_index_data(&test_tenant.tenant_db, &search, "test", 100).await;

    // Seed our special search content
    let create_file = CreateFile {
        id: SEED_FILE_ID,
        name: SEED_FILE_ID.to_string(),
        folder_id: root.id,
        ..Default::default()
    };

    let data = create_file_index(
        &create_file,
        &document_box.scope,
        Some(ProcessingIndexMetadata {
            pages: Some(vec![DocumentPage {
                page: 0,
                content: SEARCH_TEXT_CONTENT.to_string(),
            }]),
        }),
    );
    File::create(&test_tenant.tenant_db, create_file)
        .await
        .unwrap();
    search.add_data(vec![data]).await.unwrap();

    // Seed our special search content
    let create_file = CreateFile {
        id: SEED_FILE_ID_2,
        name: SEED_FILE_ID_2.to_string(),
        folder_id: root.id,
        ..Default::default()
    };

    let data = create_file_index(
        &create_file,
        &document_box.scope,
        Some(ProcessingIndexMetadata {
            pages: Some(vec![DocumentPage {
                page: 0,
                content: SEARCH_TEXT_CONTENT_2.to_string(),
            }]),
        }),
    );
    File::create(&test_tenant.tenant_db, create_file)
        .await
        .unwrap();
    search.add_data(vec![data]).await.unwrap();

    println!("finished seeding first data batch");

    seed_index_data(&test_tenant.tenant_db, &search, "test_other_scope", 50_000).await;

    println!("finished seeding data");

    const ITERATIONS: usize = 5;

    let mut iterations = Vec::new();

    for i in 0..ITERATIONS {
        let start = Instant::now();
        let results = search
            .search_index(
                &["test".to_string()],
                SearchRequest {
                    query: Some(SEARCH_TEXT_PHRASE.to_string()),
                    include_content: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();
        iterations.push(elapsed);
        println!("finished iteration {} {}ms", i, elapsed.as_millis());
        dbg!(results);
    }

    let min_duration = iterations.iter().min().unwrap();
    let max_duration = iterations.iter().max().unwrap();
    let sum_duration: Duration = iterations.iter().sum();
    let avg_duration = sum_duration / iterations.len() as u32;

    println!(
        "FIRST: min = {}ms, max = {}ms, avg = {}ms",
        min_duration.as_millis(),
        max_duration.as_millis(),
        avg_duration.as_millis()
    );

    let mut iterations = Vec::new();

    for i in 0..ITERATIONS {
        let start = Instant::now();
        let results = search
            .search_index(
                &["test".to_string()],
                SearchRequest {
                    query: Some(SEARCH_TEXT_PHRASE_2.to_string()),
                    include_content: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();
        iterations.push(elapsed);
        println!("finished iteration {} {}ms", i, elapsed.as_millis());
        dbg!(results);
    }

    let min_duration = iterations.iter().min().unwrap();
    let max_duration = iterations.iter().max().unwrap();
    let sum_duration: Duration = iterations.iter().sum();
    let avg_duration = sum_duration / iterations.len() as u32;

    println!(
        "SECOND: min = {}ms, max = {}ms, avg = {}ms",
        min_duration.as_millis(),
        max_duration.as_millis(),
        avg_duration.as_millis()
    );
}

#[tokio::test]
#[ignore = "Benchmarking test for search performance"]
async fn test_search_typesense() {
    let test_tenant = create_test_tenant().await;
    let (search, _search_container) = create_search_index_typesense(&test_tenant).await;

    println!("finished initialize");

    let (document_box, root) = seed_index_data(&test_tenant.tenant_db, &search, "test", 100).await;

    // Seed our special search content
    let create_file = CreateFile {
        id: SEED_FILE_ID,
        name: SEED_FILE_ID.to_string(),
        folder_id: root.id,
        ..Default::default()
    };

    let data = create_file_index(
        &create_file,
        &document_box.scope,
        Some(ProcessingIndexMetadata {
            pages: Some(vec![DocumentPage {
                page: 0,
                content: SEARCH_TEXT_CONTENT.to_string(),
            }]),
        }),
    );
    File::create(&test_tenant.tenant_db, create_file)
        .await
        .unwrap();
    search.add_data(vec![data]).await.unwrap();

    // Seed our special search content
    let create_file = CreateFile {
        id: SEED_FILE_ID_2,
        name: SEED_FILE_ID_2.to_string(),
        folder_id: root.id,
        ..Default::default()
    };

    let data = create_file_index(
        &create_file,
        &document_box.scope,
        Some(ProcessingIndexMetadata {
            pages: Some(vec![DocumentPage {
                page: 0,
                content: SEARCH_TEXT_CONTENT_2.to_string(),
            }]),
        }),
    );
    File::create(&test_tenant.tenant_db, create_file)
        .await
        .unwrap();
    search.add_data(vec![data]).await.unwrap();

    println!("finished seeding first data batch");

    seed_index_data(&test_tenant.tenant_db, &search, "test_other_scope", 50_000).await;

    println!("finished seeding data");

    const ITERATIONS: usize = 5;

    let mut iterations = Vec::new();

    for i in 0..ITERATIONS {
        let start = Instant::now();
        let results = search
            .search_index(
                &["test".to_string()],
                SearchRequest {
                    query: Some(SEARCH_TEXT_PHRASE.to_string()),
                    include_content: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();
        iterations.push(elapsed);
        println!("finished iteration {} {}ms", i, elapsed.as_millis());
        dbg!(results);
    }

    let min_duration = iterations.iter().min().unwrap();
    let max_duration = iterations.iter().max().unwrap();
    let sum_duration: Duration = iterations.iter().sum();
    let avg_duration = sum_duration / iterations.len() as u32;

    println!(
        "FIRST: min = {}ms, max = {}ms, avg = {}ms",
        min_duration.as_millis(),
        max_duration.as_millis(),
        avg_duration.as_millis()
    );

    let mut iterations = Vec::new();

    for i in 0..ITERATIONS {
        let start = Instant::now();
        let results = search
            .search_index(
                &["test".to_string()],
                SearchRequest {
                    query: Some(SEARCH_TEXT_PHRASE_2.to_string()),
                    include_content: true,
                    ..Default::default()
                },
                None,
            )
            .await
            .unwrap();
        let elapsed = start.elapsed();
        iterations.push(elapsed);
        println!("finished iteration {} {}ms", i, elapsed.as_millis());
        dbg!(results);
    }

    let min_duration = iterations.iter().min().unwrap();
    let max_duration = iterations.iter().max().unwrap();
    let sum_duration: Duration = iterations.iter().sum();
    let avg_duration = sum_duration / iterations.len() as u32;

    println!(
        "SECOND: min = {}ms, max = {}ms, avg = {}ms",
        min_duration.as_millis(),
        max_duration.as_millis(),
        avg_duration.as_millis()
    );
}
