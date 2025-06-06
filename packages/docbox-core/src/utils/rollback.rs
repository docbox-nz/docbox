/// "Rollback guard" pretty much a transaction, if "commit" is not called
/// and the value is dropped (Usually by an error case) the rollback
/// function will be run
pub struct RollbackGuard<F: FnOnce()> {
    rollback: Option<F>,
}

impl<F: FnOnce()> RollbackGuard<F> {
    pub fn new(rollback: F) -> Self {
        Self {
            rollback: Some(rollback),
        }
    }

    /// Explicitly cancel rollback (e.g., if all went fine).
    pub fn commit(mut self) {
        self.rollback = None;
    }
}

impl<F: FnOnce()> Drop for RollbackGuard<F> {
    fn drop(&mut self) {
        if let Some(rollback) = self.rollback.take() {
            rollback();
        }
    }
}
