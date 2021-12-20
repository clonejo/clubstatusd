use std::fmt::Write;

pub fn bytes_to_hex(bytes: &[u8]) -> String {
    let mut buf = String::new();
    for byte in bytes {
        write!(buf, "{:02x}", byte).unwrap();
    }
    buf
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_bytes_to_hex() {
        assert_eq!(bytes_to_hex(b"\x00\xff\x01"), "00ff01");
    }
}
