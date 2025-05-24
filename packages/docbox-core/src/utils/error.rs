use std::fmt;

/// Error type that contains multiple errors
#[derive(Debug)]
pub struct CompositeError(Vec<anyhow::Error>);

impl fmt::Display for CompositeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let messages = self
            .0
            .iter()
            .map(|e| format!("{e}"))
            .collect::<Vec<_>>()
            .join(", ");
        write!(f, "multiple errors occurred: {}", messages)
    }
}

impl std::error::Error for CompositeError {}

impl Extend<anyhow::Error> for CompositeError {
    fn extend<T: IntoIterator<Item = anyhow::Error>>(&mut self, iter: T) {
        self.0.extend(iter);
    }
}

impl FromIterator<anyhow::Error> for CompositeError {
    fn from_iter<T: IntoIterator<Item = anyhow::Error>>(iter: T) -> Self {
        CompositeError(iter.into_iter().collect())
    }
}
