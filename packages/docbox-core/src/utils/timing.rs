use tokio::time::{Duration, sleep};

/// Helper to run a callback when a future is taking longer than a `slow_duration`
pub async fn handle_slow_future<Fut, T, F>(future: Fut, slow_duration: Duration, callback: F) -> T
where
    Fut: Future<Output = T>,
    F: FnOnce(),
{
    tokio::pin!(future);

    let mut slow_callback = Some(callback);

    loop {
        tokio::select! {
            result = &mut future => return result,
            _ = sleep(slow_duration), if slow_callback.is_some() => {

                if let Some(slow_callback) = slow_callback.take() {
                    slow_callback();
                }
            }
        }
    }
}

#[cfg(test)]
mod test {
    use crate::utils::timing::handle_slow_future;
    use std::{sync::atomic::AtomicBool, time::Duration};
    use tokio::time::sleep;

    /// Tests that a slow future will trigger the callback
    #[tokio::test]
    async fn test_slow_future() {
        let slow = AtomicBool::new(false);

        let slow_future = sleep(Duration::from_secs(5));

        handle_slow_future(slow_future, Duration::from_secs(2), || {
            slow.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .await;

        assert!(slow.load(std::sync::atomic::Ordering::SeqCst),);
    }

    /// Tests that a fast future will not trigger the callback
    #[tokio::test]
    async fn test_fast_future() {
        let slow = AtomicBool::new(false);

        let fast_future = sleep(Duration::from_millis(1));

        handle_slow_future(fast_future, Duration::from_secs(2), || {
            slow.store(true, std::sync::atomic::Ordering::SeqCst);
        })
        .await;

        assert!(!slow.load(std::sync::atomic::Ordering::SeqCst));
    }
}
