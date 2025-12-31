use rand::distr::{Alphanumeric, SampleString};

/// Generates a random password
pub fn random_password(length: usize) -> String {
    let mut rng = rand::rng();
    Alphanumeric.sample_string(&mut rng, length)
}
