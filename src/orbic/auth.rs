use anyhow::{Context, Result};
use base64_light::base64_encode;
use serde::{Deserialize, Serialize};

fn swap_chars(s: &str, pos1: usize, pos2: usize) -> String {
    let mut chars: Vec<char> = s.chars().collect();
    if pos1 < chars.len() && pos2 < chars.len() {
        chars.swap(pos1, pos2);
    }
    chars.into_iter().collect()
}

fn apply_secret_swapping(mut text: String, secret_num: u32) -> String {
    for i in 0..4 {
        let byte = (secret_num >> (i * 8)) & 0xff;
        let pos1 = (byte as usize) % text.len();
        let pos2 = i % text.len();
        text = swap_chars(&text, pos1, pos2);
    }
    text
}

/// Encode a password using the Orbic admin API's custom algorithm.
/// Ported directly from rayhunter/installer/src/orbic_auth.rs.
pub fn encode_password(
    password: &str,
    secret: &str,
    timestamp: &str,
    timestamp_start: u64,
) -> Result<String> {
    let current_time = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();

    let password_md5 = format!("{:x}", md5::compute(password));
    let mut spliced_password = format!("a7{password_md5}");

    let secret_num =
        u32::from_str_radix(secret, 16).context("Failed to parse secret as hex")?;
    spliced_password = apply_secret_swapping(spliced_password, secret_num);

    let timestamp_hex =
        u32::from_str_radix(timestamp, 16).context("Failed to parse timestamp as hex")?;
    let time_delta = format!(
        "{:x}",
        timestamp_hex + (current_time - timestamp_start) as u32
    );

    let message = format!("6137x{time_delta}:{spliced_password}");
    let result = base64_encode(&message);
    let result = apply_secret_swapping(result, secret_num);

    Ok(result)
}

#[derive(Debug, Serialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginInfo {
    pub retcode: u32,
    #[serde(rename = "priKey")]
    pub pri_key: String,
}

#[derive(Debug, Deserialize)]
pub struct LoginResponse {
    pub retcode: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_password_produces_non_empty_string() {
        // Smoke test: the algorithm should produce a non-empty base64-ish string
        // without panicking for typical inputs.
        let result = encode_password("admin", "0000abcd", "deadbeef", 1_700_000_000);
        assert!(result.is_ok());
        assert!(!result.unwrap().is_empty());
    }
}
