use rand::{
    distributions::{Alphanumeric, DistString},
    rngs::OsRng,
};

/// Generates a random password
pub fn random_password(length: usize) -> String {
    let mut rng = OsRng;
    Alphanumeric.sample_string(&mut rng, length)
}
