use chrono::{DateTime, Utc};
use docbox_database::{
    DbPool, DbResult,
    models::{
        document_box::DocumentBoxScopeRaw,
        tasks::{Task, TaskId, TaskStatus},
    },
};
use std::{future::Future, time::Duration};
use tokio::time::sleep;
use tracing::Instrument;

pub async fn background_task<Fut>(
    db: DbPool,
    scope: DocumentBoxScopeRaw,
    future: Fut,
) -> DbResult<(TaskId, DateTime<Utc>)>
where
    Fut: Future<Output = (TaskStatus, serde_json::Value)> + Send + 'static,
{
    // Create task for progression
    let mut task = Task::create(&db, scope).await?;

    let task_id = task.id;
    let created_at = task.created_at;

    let span = tracing::Span::current();

    // Swap background task
    tokio::spawn(
        async move {
            let (status, output) = future.await;

            // Multiple retry attempts:
            // We retry multiple times because things like database connection exhaustion could
            // prevent a connection from being acquired to commit the state. But we need to make
            // sure that this state is committed
            for i in 1..5 {
                // Update task completion
                match task.complete_task(&db, status, Some(output.clone())).await {
                    Ok(_) => break,
                    Err(error) => {
                        tracing::error!(?error, "failed to mark task as complete");
                        sleep(Duration::from_secs(60 * (i * i))).await;
                    }
                }
            }
        }
        .instrument(span),
    );

    Ok((task_id, created_at))
}
