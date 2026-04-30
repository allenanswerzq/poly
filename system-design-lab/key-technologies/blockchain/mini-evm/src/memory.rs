//! # EVM Memory
//!
//! Byte-addressable, dynamically expanding memory.
//! Expansion costs gas based on memory size.

use eth_primitives::U256;
use crate::error::Result;

/// EVM Memory - byte-addressable, expandable
#[derive(Debug, Clone, Default)]
pub struct Memory {
    data: Vec<u8>,
}

impl Memory {
    /// Create new empty memory
    pub fn new() -> Self {
        Memory { data: Vec::new() }
    }

    /// Current memory size in bytes
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if memory is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Get memory size in 32-byte words (for MSIZE opcode)
    pub fn size_words(&self) -> usize {
        (self.data.len() + 31) / 32
    }

    /// Expand memory to at least `size` bytes
    /// Returns the gas cost for expansion
    pub fn expand(&mut self, size: usize) -> u64 {
        if size <= self.data.len() {
            return 0;
        }

        // Round up to 32-byte words
        let new_size = (size + 31) / 32 * 32;
        let old_words = self.size_words();

        self.data.resize(new_size, 0);

        let new_words = self.size_words();

        // Gas cost: 3 * words + words^2 / 512
        Self::memory_cost(new_words) - Self::memory_cost(old_words)
    }

    /// Calculate memory cost for a given number of words
    fn memory_cost(words: usize) -> u64 {
        let words = words as u64;
        3 * words + words * words / 512
    }

    /// Store a single byte at offset
    pub fn store8(&mut self, offset: usize, value: u8) -> u64 {
        let gas = self.expand(offset + 1);
        self.data[offset] = value;
        gas
    }

    /// Store 32 bytes at offset (MSTORE)
    pub fn store(&mut self, offset: usize, value: U256) -> u64 {
        let gas = self.expand(offset + 32);
        let bytes = value.to_be_bytes();
        self.data[offset..offset + 32].copy_from_slice(&bytes);
        gas
    }

    /// Load 32 bytes from offset (MLOAD)
    pub fn load(&mut self, offset: usize) -> (U256, u64) {
        let gas = self.expand(offset + 32);
        let mut bytes = [0u8; 32];
        bytes.copy_from_slice(&self.data[offset..offset + 32]);
        (U256::from_be_bytes(&bytes), gas)
    }

    /// Load a byte from offset
    pub fn load8(&mut self, offset: usize) -> (u8, u64) {
        let gas = self.expand(offset + 1);
        (self.data[offset], gas)
    }

    /// Copy data within memory (MCOPY)
    pub fn copy(&mut self, dest: usize, src: usize, size: usize) -> u64 {
        if size == 0 {
            return 0;
        }

        let max_offset = dest.max(src) + size;
        let gas = self.expand(max_offset);

        // Handle overlapping regions
        if dest <= src {
            for i in 0..size {
                self.data[dest + i] = self.data[src + i];
            }
        } else {
            for i in (0..size).rev() {
                self.data[dest + i] = self.data[src + i];
            }
        }

        gas
    }

    /// Copy external data into memory (CALLDATACOPY, CODECOPY, etc.)
    pub fn copy_from(&mut self, dest: usize, data: &[u8], src: usize, size: usize) -> u64 {
        if size == 0 {
            return 0;
        }

        let gas = self.expand(dest + size);

        for i in 0..size {
            let byte = if src + i < data.len() {
                data[src + i]
            } else {
                0 // Pad with zeros
            };
            self.data[dest + i] = byte;
        }

        gas
    }

    /// Get a slice of memory
    pub fn slice(&mut self, offset: usize, size: usize) -> (&[u8], u64) {
        if size == 0 {
            return (&[], 0);
        }
        let gas = self.expand(offset + size);
        (&self.data[offset..offset + size], gas)
    }

    /// Get raw data access
    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_store_load() {
        let mut mem = Memory::new();

        let value = U256::from_u64(0x1234567890abcdef);
        mem.store(0, value);

        let (loaded, _) = mem.load(0);
        assert_eq!(loaded, value);
    }

    #[test]
    fn test_store8() {
        let mut mem = Memory::new();
        mem.store8(10, 0xff);

        let (loaded, _) = mem.load8(10);
        assert_eq!(loaded, 0xff);
    }

    #[test]
    fn test_expansion() {
        let mut mem = Memory::new();
        assert_eq!(mem.len(), 0);

        mem.store(0, U256::from_u64(1));
        assert_eq!(mem.len(), 32);

        mem.store(100, U256::from_u64(2));
        assert!(mem.len() >= 132);
    }

    #[test]
    fn test_copy_from() {
        let mut mem = Memory::new();
        let data = vec![1, 2, 3, 4, 5];

        mem.copy_from(0, &data, 0, 5);
        assert_eq!(&mem.data()[0..5], &[1, 2, 3, 4, 5]);

        // Copy with padding
        mem.copy_from(10, &data, 3, 5);
        assert_eq!(&mem.data()[10..15], &[4, 5, 0, 0, 0]);
    }

    #[test]
    fn test_size_words() {
        let mut mem = Memory::new();
        assert_eq!(mem.size_words(), 0);

        mem.expand(1);
        assert_eq!(mem.size_words(), 1);

        mem.expand(32);
        assert_eq!(mem.size_words(), 1);

        mem.expand(33);
        assert_eq!(mem.size_words(), 2);
    }
}
