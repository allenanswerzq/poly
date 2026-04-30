//! # RLP (Recursive Length Prefix) Encoding
//!
//! Ethereum's serialization format for data structures.
//!
//! Rules:
//! - Single byte < 0x80: encoded as itself
//! - String 0-55 bytes: 0x80 + length, then bytes
//! - String > 55 bytes: 0xb7 + length-of-length, length as big-endian, then bytes
//! - List 0-55 bytes total: 0xc0 + length, then concatenated RLP items
//! - List > 55 bytes total: 0xf7 + length-of-length, length as big-endian, then items

use crate::error::{EthError, Result};

/// RLP-encodable trait
pub trait Encodable {
    fn rlp_encode(&self) -> Vec<u8>;
}

/// RLP-decodable trait
pub trait Decodable: Sized {
    fn rlp_decode(data: &[u8]) -> Result<(Self, usize)>;
}

/// Encode a single byte string/bytes
pub fn encode_bytes(data: &[u8]) -> Vec<u8> {
    if data.len() == 1 && data[0] < 0x80 {
        // Single byte below 0x80: encode as itself
        vec![data[0]]
    } else if data.len() <= 55 {
        // Short string: 0x80 + length prefix
        let mut result = vec![0x80 + data.len() as u8];
        result.extend_from_slice(data);
        result
    } else {
        // Long string: 0xb7 + length-of-length
        let len_bytes = encode_length(data.len());
        let mut result = vec![0xb7 + len_bytes.len() as u8];
        result.extend_from_slice(&len_bytes);
        result.extend_from_slice(data);
        result
    }
}

/// Encode a list of already-encoded items
pub fn encode_list(items: &[Vec<u8>]) -> Vec<u8> {
    let concatenated: Vec<u8> = items.iter().flatten().copied().collect();

    if concatenated.len() <= 55 {
        // Short list: 0xc0 + length prefix
        let mut result = vec![0xc0 + concatenated.len() as u8];
        result.extend_from_slice(&concatenated);
        result
    } else {
        // Long list: 0xf7 + length-of-length
        let len_bytes = encode_length(concatenated.len());
        let mut result = vec![0xf7 + len_bytes.len() as u8];
        result.extend_from_slice(&len_bytes);
        result.extend_from_slice(&concatenated);
        result
    }
}

/// Encode length as big-endian bytes (minimal encoding)
fn encode_length(len: usize) -> Vec<u8> {
    if len == 0 {
        return vec![];
    }

    let mut result = Vec::new();
    let mut remaining = len;
    while remaining > 0 {
        result.push((remaining & 0xff) as u8);
        remaining >>= 8;
    }
    result.reverse();
    result
}

/// Decode RLP bytes into (item, bytes_consumed)
pub fn decode_bytes(data: &[u8]) -> Result<(Vec<u8>, usize)> {
    if data.is_empty() {
        return Err(EthError::RlpError("Empty input".into()));
    }

    let prefix = data[0];

    if prefix < 0x80 {
        // Single byte
        Ok((vec![prefix], 1))
    } else if prefix <= 0xb7 {
        // Short string (0-55 bytes)
        let len = (prefix - 0x80) as usize;
        if data.len() < 1 + len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        Ok((data[1..1 + len].to_vec(), 1 + len))
    } else if prefix <= 0xbf {
        // Long string
        let len_of_len = (prefix - 0xb7) as usize;
        if data.len() < 1 + len_of_len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        let len = decode_length(&data[1..1 + len_of_len])?;
        if data.len() < 1 + len_of_len + len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        Ok((
            data[1 + len_of_len..1 + len_of_len + len].to_vec(),
            1 + len_of_len + len,
        ))
    } else {
        // It's a list prefix, not bytes
        Err(EthError::RlpError("Expected bytes, got list".into()))
    }
}

/// Decode RLP list into vector of items
pub fn decode_list(data: &[u8]) -> Result<(Vec<Vec<u8>>, usize)> {
    if data.is_empty() {
        return Err(EthError::RlpError("Empty input".into()));
    }

    let prefix = data[0];

    if prefix < 0xc0 {
        return Err(EthError::RlpError("Expected list, got bytes".into()));
    }

    let (list_data, total_consumed) = if prefix <= 0xf7 {
        // Short list
        let len = (prefix - 0xc0) as usize;
        if data.len() < 1 + len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        (&data[1..1 + len], 1 + len)
    } else {
        // Long list
        let len_of_len = (prefix - 0xf7) as usize;
        if data.len() < 1 + len_of_len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        let len = decode_length(&data[1..1 + len_of_len])?;
        if data.len() < 1 + len_of_len + len {
            return Err(EthError::RlpError("Input too short".into()));
        }
        (&data[1 + len_of_len..1 + len_of_len + len], 1 + len_of_len + len)
    };

    // Decode items within the list
    let mut items = Vec::new();
    let mut offset = 0;

    while offset < list_data.len() {
        let item_prefix = list_data[offset];

        if item_prefix < 0xc0 {
            // It's bytes
            let (item, consumed) = decode_bytes(&list_data[offset..])?;
            items.push(item);
            offset += consumed;
        } else {
            // It's a nested list - for simplicity, return raw bytes
            let (_, consumed) = decode_list(&list_data[offset..])?;
            items.push(list_data[offset..offset + consumed].to_vec());
            offset += consumed;
        }
    }

    Ok((items, total_consumed))
}

/// Decode big-endian length bytes
fn decode_length(data: &[u8]) -> Result<usize> {
    let mut result: usize = 0;
    for byte in data {
        result = result.checked_mul(256)
            .ok_or_else(|| EthError::RlpError("Length overflow".into()))?;
        result = result.checked_add(*byte as usize)
            .ok_or_else(|| EthError::RlpError("Length overflow".into()))?;
    }
    Ok(result)
}

// Implement Encodable for common types
impl Encodable for Vec<u8> {
    fn rlp_encode(&self) -> Vec<u8> {
        encode_bytes(self)
    }
}

impl Encodable for &[u8] {
    fn rlp_encode(&self) -> Vec<u8> {
        encode_bytes(self)
    }
}

impl Encodable for u64 {
    fn rlp_encode(&self) -> Vec<u8> {
        if *self == 0 {
            encode_bytes(&[])
        } else {
            // Encode as big-endian with no leading zeros
            let bytes = self.to_be_bytes();
            let start = bytes.iter().position(|&b| b != 0).unwrap_or(7);
            encode_bytes(&bytes[start..])
        }
    }
}

impl Encodable for crate::U256 {
    fn rlp_encode(&self) -> Vec<u8> {
        if self.is_zero() {
            encode_bytes(&[])
        } else {
            let bytes = self.to_be_bytes();
            let start = bytes.iter().position(|&b| b != 0).unwrap_or(31);
            encode_bytes(&bytes[start..])
        }
    }
}

impl Encodable for crate::Address {
    fn rlp_encode(&self) -> Vec<u8> {
        encode_bytes(&self.0)
    }
}

impl Encodable for crate::H256 {
    fn rlp_encode(&self) -> Vec<u8> {
        encode_bytes(&self.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encode_single_byte() {
        assert_eq!(encode_bytes(&[0x00]), vec![0x00]);
        assert_eq!(encode_bytes(&[0x7f]), vec![0x7f]);
        // 0x80 and above need prefix
        assert_eq!(encode_bytes(&[0x80]), vec![0x81, 0x80]);
    }

    #[test]
    fn test_encode_short_string() {
        assert_eq!(encode_bytes(b"dog"), vec![0x83, b'd', b'o', b'g']);
        assert_eq!(encode_bytes(&[]), vec![0x80]); // Empty string
    }

    #[test]
    fn test_encode_long_string() {
        // 56 bytes
        let data = vec![0xaa; 56];
        let encoded = encode_bytes(&data);
        assert_eq!(encoded[0], 0xb8); // 0xb7 + 1 (1 byte for length)
        assert_eq!(encoded[1], 56);   // Length
        assert_eq!(&encoded[2..], data.as_slice());
    }

    #[test]
    fn test_encode_empty_list() {
        let encoded = encode_list(&[]);
        assert_eq!(encoded, vec![0xc0]);
    }

    #[test]
    fn test_encode_list() {
        // ["cat", "dog"]
        let items = vec![
            encode_bytes(b"cat"),
            encode_bytes(b"dog"),
        ];
        let encoded = encode_list(&items);
        // 0xc8 = 0xc0 + 8 (total length of encoded items)
        assert_eq!(encoded, vec![0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g']);
    }

    #[test]
    fn test_decode_bytes() {
        // Single byte
        let (data, consumed) = decode_bytes(&[0x42]).unwrap();
        assert_eq!(data, vec![0x42]);
        assert_eq!(consumed, 1);

        // Short string
        let (data, consumed) = decode_bytes(&[0x83, b'd', b'o', b'g']).unwrap();
        assert_eq!(data, b"dog".to_vec());
        assert_eq!(consumed, 4);
    }

    #[test]
    fn test_decode_list() {
        let encoded = vec![0xc8, 0x83, b'c', b'a', b't', 0x83, b'd', b'o', b'g'];
        let (items, consumed) = decode_list(&encoded).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], b"cat".to_vec());
        assert_eq!(items[1], b"dog".to_vec());
        assert_eq!(consumed, 9);
    }

    #[test]
    fn test_roundtrip() {
        let original = b"hello world";
        let encoded = encode_bytes(original);
        let (decoded, _) = decode_bytes(&encoded).unwrap();
        assert_eq!(decoded, original.to_vec());
    }

    #[test]
    fn test_encode_u64() {
        assert_eq!(0u64.rlp_encode(), vec![0x80]); // Empty string for 0
        assert_eq!(127u64.rlp_encode(), vec![0x7f]); // Single byte
        assert_eq!(128u64.rlp_encode(), vec![0x81, 0x80]); // Needs prefix
        assert_eq!(1024u64.rlp_encode(), vec![0x82, 0x04, 0x00]); // 2 bytes
    }
}
