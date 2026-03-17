//! 工具函数模块

pub mod sql;

use ring::digest;

/// 计算 SHA-256 哈希
pub fn sha256(data: &[u8]) -> [u8; 32] {
    let digest = digest::digest(&digest::SHA256, data);
    let mut result = [0u8; 32];
    result.copy_from_slice(digest.as_ref());
    result
}

/// 安全比较（防时序攻击）
pub fn secure_compare(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    let mut result = 0u8;
    for (x, y) in a.iter().zip(b.iter()) {
        result |= x ^ y;
    }
    result == 0
}

/// 获取当前时间戳（秒）
pub fn current_timestamp() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("系统时间错误")
        .as_secs()
}

/// 字节数组转十六进制字符串
pub fn bytes_to_hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut result = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(&mut result, "{:02x}", byte).unwrap();
    }
    result
}

/// 十六进制字符串转字节数组
pub fn hex_to_bytes(hex: &str) -> Result<Vec<u8>, String> {
    if hex.len() % 2 != 0 {
        return Err("Invalid hex length".to_string());
    }

    let mut result = Vec::with_capacity(hex.len() / 2);
    for i in (0..hex.len()).step_by(2) {
        let byte = u8::from_str_radix(&hex[i..i + 2], 16)
            .map_err(|_| "Invalid hex character".to_string())?;
        result.push(byte);
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sha256() {
        let data = b"hello world";
        let hash = sha256(data);
        assert_eq!(hash.len(), 32);

        // 相同的输入应该产生相同的哈希
        let hash2 = sha256(data);
        assert_eq!(hash, hash2);

        // 不同的输入应该产生不同的哈希
        let hash3 = sha256(b"different");
        assert_ne!(hash, hash3);
    }

    #[test]
    fn test_secure_compare() {
        let a = b"secret";
        let b = b"secret";
        let c = b"different";

        assert!(secure_compare(a, b));
        assert!(!secure_compare(a, c));
        assert!(!secure_compare(a, &c[..3]));
    }

    #[test]
    fn test_hex_encoding() {
        let bytes = vec![0xde, 0xad, 0xbe, 0xef];
        let hex = bytes_to_hex(&bytes);
        assert_eq!(hex, "deadbeef");

        let decoded = hex_to_bytes(&hex).unwrap();
        assert_eq!(decoded, bytes);
    }

    #[test]
    fn test_hex_invalid_length() {
        let result = hex_to_bytes("abc");
        assert!(result.is_err());
    }

    #[test]
    fn test_hex_invalid_char() {
        let result = hex_to_bytes("gg");
        assert!(result.is_err());
    }
}
