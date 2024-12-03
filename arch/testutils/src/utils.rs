use rand::{distributions::Uniform, Rng};
use sha256::digest;

pub fn generate_upper_case_string(size: usize) -> String {
    return rand::thread_rng()
        .sample_iter(&Uniform::new(char::from(65), char::from(90)))
        .take(size)
        .map(char::from)
        .collect();
}

pub fn hash(data: &[u8]) -> String {
    digest(data)
}
