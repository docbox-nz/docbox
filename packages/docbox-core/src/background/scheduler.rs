//! # Scheduler
//!
//! Provides background task scheduling to run tasks at specific
//! fixed intervals independent of when the task started

use chrono::Local;
use futures::{Stream, future::BoxFuture};
use std::{
    collections::BinaryHeap,
    future::Future,
    pin::Pin,
    task::{Poll, ready},
    time::Duration,
};
use tokio::time::{Instant, sleep_until};

pub struct ScheduledEvent<E> {
    /// Data for the event to execute
    pub event: E,

    /// Interval the event executes at in seconds
    /// (For further scheduling)
    pub interval: u64,

    /// Next instance the
    pub next_run: Instant,
}

impl<E> Eq for ScheduledEvent<E> {}

impl<E> PartialEq for ScheduledEvent<E> {
    fn eq(&self, other: &Self) -> bool {
        self.next_run.eq(&other.next_run)
    }
}

impl<E> PartialOrd for ScheduledEvent<E> {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl<E> Ord for ScheduledEvent<E> {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Reverse comparison order for binary heap to sort
        // closest ones to the top
        other.next_run.cmp(&self.next_run)
    }
}

pub struct SchedulerQueueEvent<E> {
    /// Data for the event
    pub event: E,
    /// Interval the event executes at in seconds
    pub interval: u64,
}

pub struct SchedulerEventStream<E> {
    /// Heap of scheduled events, ordered by the event which is
    /// due to come first
    events: BinaryHeap<ScheduledEvent<E>>,

    /// Current sleep future
    current_sleep: Option<BoxFuture<'static, ()>>,
}

impl<E> SchedulerEventStream<E>
where
    E: Clone + Unpin + PartialEq + Ord + 'static,
{
    pub fn new(events: Vec<SchedulerQueueEvent<E>>) -> SchedulerEventStream<E>
    where
        E: Clone + PartialEq + Ord + 'static,
    {
        SchedulerEventStream {
            events: events
                .into_iter()
                .map(|event| create_scheduled_event(event.event, event.interval))
                .collect(),
            current_sleep: None,
        }
    }

    /// Takes the current event pushing its next iteration to the
    /// event heap then returns the current value
    fn reschedule_current_event(&mut self) -> Option<E> {
        let event = self.events.pop()?;

        // Create the next iteration of the event
        self.events
            .push(create_scheduled_event(event.event.clone(), event.interval));

        // Emit event
        Some(event.event)
    }
}

impl<E> Stream for SchedulerEventStream<E>
where
    E: Clone + Unpin + PartialEq + Ord + 'static,
{
    type Item = E;

    fn poll_next(
        self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Self::Item>> {
        let this = self.get_mut();

        loop {
            if let Some(current_sleep) = this.current_sleep.as_mut() {
                // Poll current sleep
                if Pin::new(current_sleep).poll(cx).is_pending() {
                    return Poll::Pending;
                }

                // Clear current sleep
                this.current_sleep = None;

                return match this.reschedule_current_event() {
                    Some(event) => Poll::Ready(Some(event)),
                    None => Poll::Pending,
                };
            }

            // Peek the top event
            let next_event = match this.events.peek() {
                Some(value) => value,
                None => return Poll::Pending,
            };

            // Check if the event has already passed
            let now = Instant::now();
            if next_event.next_run < now {
                return match this.reschedule_current_event() {
                    Some(event) => Poll::Ready(Some(event)),
                    None => Poll::Pending,
                };
            }

            // Store and poll new sleep state
            let sleep = sleep_until(next_event.next_run);
            let sleep = this.current_sleep.insert(Box::pin(sleep));
            ready!(Pin::new(sleep).poll(cx));
        }
    }
}

fn create_scheduled_event<E>(event: E, interval: u64) -> ScheduledEvent<E> {
    let next_run = get_nth_interval_instant(interval, 1);
    ScheduledEvent {
        event,
        interval,
        next_run,
    }
}

/// Gets the next instant for a fixed interval in seconds
fn get_nth_interval_instant(interval: u64, nth: u64) -> Instant {
    let now = Local::now();
    let seconds_since_epoch = now.timestamp() as u64;
    let next = (seconds_since_epoch / interval + nth) * interval;
    Instant::now() + Duration::from_secs(next - seconds_since_epoch)
}

#[cfg(test)]
mod test {
    use futures::{FutureExt, StreamExt};
    use std::sync::{Arc, Mutex};
    use tokio::{spawn, time::sleep_until};

    use crate::background::scheduler::{SchedulerEventStream, get_nth_interval_instant};

    use super::SchedulerQueueEvent;

    /// Tests that the correct number of events is produced over time
    #[tokio::test]
    async fn test_event_production() {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
        struct MyEvent;

        let events = vec![SchedulerQueueEvent {
            event: MyEvent,
            interval: 1,
        }];

        let start_instant = get_nth_interval_instant(1, 1);
        let mut scheduler = SchedulerEventStream::new(events);

        // Consume any events that were made immediately
        scheduler.next().now_or_never();

        let events: Arc<Mutex<Vec<MyEvent>>> = Default::default();

        spawn({
            let events = events.clone();
            async move {
                while let Some(event) = scheduler.next().await {
                    events.lock().expect("lock was poisoned").push(event);
                }
            }
        });

        // Sleep until the start point, should have no events at this point
        sleep_until(start_instant).await;
        assert_eq!(events.lock().unwrap().len(), 0);

        // Repeat testing 5 times to ensure correctness increasing the number
        // of events that should have elapsed
        for nth in 1..6 {
            // Get the 5th interval from now
            let start_instant = get_nth_interval_instant(1, nth);

            {
                // Sleep until the start point, should have nth events at this point
                sleep_until(start_instant).await;
                assert_eq!(events.lock().unwrap().len(), nth as usize);
            }

            // Reset for next iteration
            events.lock().unwrap().clear();
        }
    }
}
