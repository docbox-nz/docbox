use docbox_database::{
    DbErr, DbPool,
    models::{
        file::File,
        folder::{Folder, ResolvedFolder},
        link::Link,
    },
};
use futures::{FutureExt, Stream, future::BoxFuture};
use std::{collections::VecDeque, task::Poll};
use thiserror::Error;

/// Item produced by [FolderWalkStream]
#[derive(Debug)]
pub enum FolderWalkItem {
    Folder(Folder),
    File(File),
    Link(Link),
}

/// Item within the resolution stack for the folder walk stream
enum FolderWalkStackItem<'db> {
    /// Unresolved folder yet to be processed
    Unresolved(Folder),
    /// Folder resolve operating that is currently undergoing
    Resolving {
        /// The folder
        folder: Folder,
        /// Current future for resolving the contents of the folder
        future: BoxFuture<'db, Result<ResolvedFolder, DbErr>>,
    },
    /// Resolved item that can be returned
    Resolved(FolderWalkItem),
}

/// Error that can occur when walking the folder
#[derive(Debug, Error)]
pub enum FolderWalkError {
    #[error("failed to resolve folder")]
    ResolveFolder(DbErr),
}

/// Stream for walking a folder starting with a top most folder
/// producing each of the folders children walking depth first
///
/// This stream will always produce the deepest content first
/// in order of links -> files -> folder it is safe to use
/// this stream to perform deletions of a folder as the deepest
/// folder contents will always be produced before the folder itself
pub struct FolderWalkStream<'db> {
    /// Database pool for resolving folders
    db: &'db DbPool,

    /// Stack of items to process
    stack: VecDeque<FolderWalkStackItem<'db>>,
}

impl<'db> FolderWalkStream<'db> {
    pub fn new(db: &'db DbPool, folder: Folder) -> FolderWalkStream<'db> {
        let mut stack = VecDeque::new();
        stack.push_back(FolderWalkStackItem::Unresolved(folder));

        FolderWalkStream { db, stack }
    }
}

impl<'db> Stream for FolderWalkStream<'db> {
    type Item = Result<FolderWalkItem, FolderWalkError>;

    fn poll_next(
        self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        while let Some(item) = this.stack.pop_front() {
            match item {
                // Currently resolving a folder
                FolderWalkStackItem::Resolving { folder, mut future } => {
                    let resolved = match future.poll_unpin(cx) {
                        // Folder is resolved and can be processed
                        Poll::Ready(Ok(value)) => value,

                        // Error encountered while iterating
                        Poll::Ready(Err(error)) => {
                            return Poll::Ready(Some(Err(FolderWalkError::ResolveFolder(error))));
                        }

                        // Still waiting for the completed resolve push back onto the stack
                        Poll::Pending => {
                            this.stack
                                .push_front(FolderWalkStackItem::Resolving { folder, future });
                            return Poll::Pending;
                        }
                    };

                    // Folder is now resolved and can be pushed to the stack
                    this.stack
                        .push_front(FolderWalkStackItem::Resolved(FolderWalkItem::Folder(
                            folder,
                        )));

                    // Nested folders need to be resolved
                    for item in resolved.folders {
                        this.stack.push_front(FolderWalkStackItem::Unresolved(item));
                    }

                    // Files and links can now be resolved
                    for item in resolved.files {
                        this.stack
                            .push_front(FolderWalkStackItem::Resolved(FolderWalkItem::File(item)));
                    }

                    for item in resolved.links {
                        this.stack
                            .push_front(FolderWalkStackItem::Resolved(FolderWalkItem::Link(item)));
                    }
                }

                // Next folder to resolve
                FolderWalkStackItem::Unresolved(folder) => {
                    let future = Box::pin(ResolvedFolder::resolve(this.db, folder.id));
                    this.stack
                        .push_front(FolderWalkStackItem::Resolving { folder, future });
                }

                // Stack item to return
                FolderWalkStackItem::Resolved(item) => {
                    return Poll::Ready(Some(Ok(item)));
                }
            }
        }

        // Reached the end of the content
        Poll::Ready(None)
    }
}
