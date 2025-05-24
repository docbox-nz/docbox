use super::{NotificationQueue, NotificationQueueMessage};
use crate::aws::SqsClient;
use std::time::Duration;
use tokio::{spawn, sync::mpsc, time::sleep};

pub struct SqsNotificationQueue {
    rx: mpsc::Receiver<NotificationQueueMessage>,
}

impl SqsNotificationQueue {
    pub fn create(client: SqsClient, queue_url: String) -> SqsNotificationQueue {
        let (tx, rx) = mpsc::channel(10);
        let task = SqsNotificationQueueTask {
            client,
            queue_url,
            tx,
        };

        spawn(process_sqs_queue(task));
        SqsNotificationQueue { rx }
    }
}

impl NotificationQueue for SqsNotificationQueue {
    async fn next_message(&mut self) -> Option<NotificationQueueMessage> {
        self.rx.recv().await
    }
}

pub struct SqsNotificationQueueTask {
    /// Underlying client to request messages from
    client: SqsClient,

    /// URL of the queue containing notifications
    queue_url: String,

    /// Sender for sending of messages that are ready
    tx: mpsc::Sender<NotificationQueueMessage>,
}

pub fn parse_bucket_message(value: &serde_json::Value) -> Option<(String, String)> {
    let records = value.get("Records")?;
    let record = records.get(0)?;

    let s3 = record.get("s3")?;
    let bucket = s3.get("bucket")?;
    let object = s3.get("object")?;

    let bucket_name = bucket.get("name")?.as_str()?.to_string();
    let object_key = object.get("key")?.as_str()?.to_string();

    Some((bucket_name, object_key))
}

async fn process_sqs_queue(task: SqsNotificationQueueTask) {
    loop {
        // Receive messages from the SQS queue
        let receive_messages = match task
            .client
            .receive_message()
            .queue_url(&task.queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(5)
            .send()
            .await
        {
            Ok(value) => value,
            Err(cause) => {
                tracing::error!(?cause, "error getting messages from sqs");
                sleep(Duration::from_secs(10)).await;
                continue;
            }
        };

        let messages = match receive_messages.messages {
            Some(value) => value,
            None => {
                tracing::debug!("no messages from sqs");
                sleep(Duration::from_secs(1)).await;
                continue;
            }
        };

        for message in messages {
            let (body, receipt_handle) = match (message.body, message.receipt_handle) {
                (Some(body), Some(receipt_handle)) => (body, receipt_handle),
                _ => continue,
            };

            let parsed: serde_json::Value = match serde_json::from_str(&body) {
                Ok(value) => value,
                Err(cause) => {
                    if let Err(cause) = task
                        .client
                        .delete_message()
                        .queue_url(&task.queue_url)
                        .receipt_handle(receipt_handle)
                        .send()
                        .await
                    {
                        tracing::error!(?cause, "failed to delete message from sqs");
                    }

                    tracing::error!(?cause, "got malformed message from sqs");
                    continue;
                }
            };

            tracing::debug!(?parsed, "got message from sqs");

            if let Some((bucket_name, object_key)) = parse_bucket_message(&parsed) {
                tracing::debug!(?bucket_name, ?object_key, "got file upload message");

                _ = task
                    .tx
                    .send(NotificationQueueMessage::FileCreated {
                        bucket_name,
                        object_key,
                    })
                    .await;
            }

            if let Err(cause) = task
                .client
                .delete_message()
                .queue_url(&task.queue_url)
                .receipt_handle(receipt_handle)
                .send()
                .await
            {
                tracing::error!(?cause, "failed to delete message from sqs");
            }
        }
    }
}
