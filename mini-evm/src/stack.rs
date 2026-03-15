//! # EVM Stack
//!
//! The EVM stack holds up to 1024 256-bit values.
//! Operations: push, pop, dup, swap

use eth_primitives::U256;
use crate::error::{EvmError, Result};

/// Maximum stack depth
pub const MAX_STACK_SIZE: usize = 1024;

/// EVM Stack (1024 x U256)
#[derive(Debug, Clone)]
pub struct Stack {
    data: Vec<U256>,
}

impl Default for Stack {
    fn default() -> Self {
        Self::new()
    }
}

impl Stack {
    /// Create a new empty stack
    pub fn new() -> Self {
        Stack {
            data: Vec::with_capacity(64), // Pre-allocate some space
        }
    }

    /// Current stack size
    pub fn len(&self) -> usize {
        self.data.len()
    }

    /// Check if stack is empty
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Push a value onto the stack
    pub fn push(&mut self, value: U256) -> Result<()> {
        if self.data.len() >= MAX_STACK_SIZE {
            return Err(EvmError::StackOverflow);
        }
        self.data.push(value);
        Ok(())
    }

    /// Pop a value from the stack
    pub fn pop(&mut self) -> Result<U256> {
        self.data.pop().ok_or(EvmError::StackUnderflow)
    }

    /// Peek at the top value without removing it
    pub fn peek(&self) -> Result<U256> {
        self.data.last().copied().ok_or(EvmError::StackUnderflow)
    }

    /// Peek at a value at a specific depth (0 = top)
    pub fn peek_at(&self, depth: usize) -> Result<U256> {
        if depth >= self.data.len() {
            return Err(EvmError::StackUnderflow);
        }
        Ok(self.data[self.data.len() - 1 - depth])
    }

    /// Set a value at a specific depth (0 = top)
    pub fn set_at(&mut self, depth: usize, value: U256) -> Result<()> {
        if depth >= self.data.len() {
            return Err(EvmError::StackUnderflow);
        }
        let idx = self.data.len() - 1 - depth;
        self.data[idx] = value;
        Ok(())
    }

    /// Duplicate the nth item (1-indexed, DUP1 duplicates top)
    pub fn dup(&mut self, n: usize) -> Result<()> {
        if n == 0 || n > 16 {
            return Err(EvmError::StackUnderflow);
        }
        let value = self.peek_at(n - 1)?;
        self.push(value)
    }

    /// Swap top with nth item (1-indexed, SWAP1 swaps top two)
    pub fn swap(&mut self, n: usize) -> Result<()> {
        if n == 0 || n > 16 {
            return Err(EvmError::StackUnderflow);
        }
        if self.data.len() <= n {
            return Err(EvmError::StackUnderflow);
        }

        let top_idx = self.data.len() - 1;
        let swap_idx = self.data.len() - 1 - n;
        self.data.swap(top_idx, swap_idx);
        Ok(())
    }

    /// Pop two values for binary operations
    pub fn pop2(&mut self) -> Result<(U256, U256)> {
        let a = self.pop()?;
        let b = self.pop()?;
        Ok((a, b))
    }

    /// Pop three values for ternary operations
    pub fn pop3(&mut self) -> Result<(U256, U256, U256)> {
        let a = self.pop()?;
        let b = self.pop()?;
        let c = self.pop()?;
        Ok((a, b, c))
    }

    /// Clear the stack
    pub fn clear(&mut self) {
        self.data.clear();
    }

    /// Get all values (for debugging)
    pub fn values(&self) -> &[U256] {
        &self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_pop() {
        let mut stack = Stack::new();
        stack.push(U256::from_u64(1)).unwrap();
        stack.push(U256::from_u64(2)).unwrap();

        assert_eq!(stack.len(), 2);
        assert_eq!(stack.pop().unwrap(), U256::from_u64(2));
        assert_eq!(stack.pop().unwrap(), U256::from_u64(1));
        assert!(stack.pop().is_err());
    }

    #[test]
    fn test_dup() {
        let mut stack = Stack::new();
        stack.push(U256::from_u64(1)).unwrap();
        stack.push(U256::from_u64(2)).unwrap();
        stack.push(U256::from_u64(3)).unwrap();

        // DUP1: duplicate top (3)
        stack.dup(1).unwrap();
        assert_eq!(stack.peek().unwrap(), U256::from_u64(3));
        assert_eq!(stack.len(), 4);

        // DUP3: duplicate 3rd from top (still 2)
        stack.dup(3).unwrap();
        assert_eq!(stack.peek().unwrap(), U256::from_u64(2));
    }

    #[test]
    fn test_swap() {
        let mut stack = Stack::new();
        stack.push(U256::from_u64(1)).unwrap();
        stack.push(U256::from_u64(2)).unwrap();
        stack.push(U256::from_u64(3)).unwrap();

        // SWAP1: swap top two (3 and 2)
        stack.swap(1).unwrap();
        assert_eq!(stack.pop().unwrap(), U256::from_u64(2));
        assert_eq!(stack.pop().unwrap(), U256::from_u64(3));
        assert_eq!(stack.pop().unwrap(), U256::from_u64(1));
    }

    #[test]
    fn test_overflow() {
        let mut stack = Stack::new();
        for i in 0..MAX_STACK_SIZE {
            stack.push(U256::from_u64(i as u64)).unwrap();
        }
        assert!(stack.push(U256::from_u64(9999)).is_err());
    }
}
