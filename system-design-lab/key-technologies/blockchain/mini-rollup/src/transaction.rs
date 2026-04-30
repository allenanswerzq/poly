//! # L2 Transaction
//!
//! Transactions on the rollup layer

use eth_primitives::{Address, H256, keccak256, Signature};

/// L2 Transaction
#[derive(Debug, Clone)]
pub struct L2Transaction {
    /// Sender address
    pub from: Address,
    /// Recipient address (None for contract creation)
    pub to: Option<Address>,
    /// Value in wei
    pub value: u64,
    /// Transaction data (calldata)
    pub data: Vec<u8>,
    /// Sender's nonce
    pub nonce: u64,
    /// Gas limit
    pub gas_limit: u64,
    /// Max fee per gas
    pub max_fee: u64,
    /// Signature
    pub signature: Option<Signature>,
}

/// Transaction type
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionType {
    Transfer,
    ContractCall,
    ContractCreate,
}

impl L2Transaction {
    /// Create new transaction
    pub fn new(
        from: Address,
        to: Option<Address>,
        value: u64,
        data: Vec<u8>,
        nonce: u64,
    ) -> Self {
        L2Transaction {
            from,
            to,
            value,
            data,
            nonce,
            gas_limit: 21000,
            max_fee: 1_000_000_000, // 1 gwei
            signature: None,
        }
    }

    /// Create simple transfer
    pub fn transfer(from: Address, to: Address, value: u64, nonce: u64) -> Self {
        L2Transaction::new(from, Some(to), value, Vec::new(), nonce)
    }

    /// Compute transaction hash
    pub fn hash(&self) -> H256 {
        let mut data = Vec::new();
        data.extend_from_slice(self.from.as_bytes());
        if let Some(to) = &self.to {
            data.extend_from_slice(to.as_bytes());
        }
        data.extend_from_slice(&self.value.to_be_bytes());
        data.extend_from_slice(&self.nonce.to_be_bytes());
        data.extend_from_slice(&self.data);

        keccak256(&data)
    }

    /// Compute data to sign
    pub fn signing_hash(&self) -> H256 {
        // In real rollup, would use EIP-712 typed data
        self.hash()
    }

    /// Estimate gas for this transaction
    pub fn estimate_gas(&self) -> u64 {
        // Base cost + data cost
        let base = 21000u64;
        let data_cost = self.data.iter()
            .map(|&b| if b == 0 { 4u64 } else { 16u64 })
            .sum::<u64>();
        base + data_cost
    }

    /// Get compressed representation for batching
    pub fn compress(&self) -> Vec<u8> {
        // Simplified compression - real rollups use more efficient encoding
        let mut compressed = Vec::new();
        compressed.extend_from_slice(self.from.as_bytes());
        if let Some(to) = &self.to {
            compressed.push(1); // has recipient
            compressed.extend_from_slice(to.as_bytes());
        } else {
            compressed.push(0); // contract creation
        }
        compressed.extend_from_slice(&self.value.to_be_bytes());
        compressed.extend_from_slice(&self.nonce.to_be_bytes());
        compressed.extend_from_slice(&(self.data.len() as u32).to_be_bytes());
        compressed.extend_from_slice(&self.data);
        compressed
    }

    /// Decompress transaction
    pub fn decompress(data: &[u8]) -> Option<Self> {
        if data.len() < 37 {
            return None;
        }

        let mut from_bytes = [0u8; 20];
        from_bytes.copy_from_slice(&data[0..20]);
        let from = Address::new(from_bytes);

        let mut pos = 20;
        let to = if data[pos] == 1 {
            pos += 1;
            let mut to_bytes = [0u8; 20];
            to_bytes.copy_from_slice(&data[pos..pos+20]);
            pos += 20;
            Some(Address::new(to_bytes))
        } else {
            pos += 1;
            None
        };

        let mut value_bytes = [0u8; 8];
        value_bytes.copy_from_slice(&data[pos..pos+8]);
        let value = u64::from_be_bytes(value_bytes);
        pos += 8;

        let mut nonce_bytes = [0u8; 8];
        nonce_bytes.copy_from_slice(&data[pos..pos+8]);
        let nonce = u64::from_be_bytes(nonce_bytes);
        pos += 8;

        let mut len_bytes = [0u8; 4];
        len_bytes.copy_from_slice(&data[pos..pos+4]);
        let data_len = u32::from_be_bytes(len_bytes) as usize;
        pos += 4;

        let tx_data = data[pos..pos+data_len].to_vec();

        Some(L2Transaction::new(from, to, value, tx_data, nonce))
    }
}

/// Transaction execution result
#[derive(Debug, Clone)]
pub struct TransactionResult {
    /// Transaction hash
    pub tx_hash: H256,
    /// Success or failure
    pub success: bool,
    /// Gas used
    pub gas_used: u64,
    /// Error message if failed
    pub error: Option<String>,
    /// Return data
    pub return_data: Vec<u8>,
}

impl TransactionResult {
    pub fn success(tx_hash: H256, gas_used: u64) -> Self {
        TransactionResult {
            tx_hash,
            success: true,
            gas_used,
            error: None,
            return_data: Vec::new(),
        }
    }

    pub fn failure(tx_hash: H256, gas_used: u64, error: String) -> Self {
        TransactionResult {
            tx_hash,
            success: false,
            gas_used,
            error: Some(error),
            return_data: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transaction_hash() {
        let from = Address::from_hex("0x1234567890123456789012345678901234567890").unwrap();
        let to = Address::from_hex("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap();

        let tx1 = L2Transaction::transfer(from, to, 1000, 0);
        let tx2 = L2Transaction::transfer(from, to, 1000, 0);
        let tx3 = L2Transaction::transfer(from, to, 1001, 0);

        // Same parameters = same hash
        assert_eq!(tx1.hash(), tx2.hash());

        // Different value = different hash
        assert_ne!(tx1.hash(), tx3.hash());
    }

    #[test]
    fn test_compress_decompress() {
        let from = Address::from_hex("0x1234567890123456789012345678901234567890").unwrap();
        let to = Address::from_hex("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap();

        let tx = L2Transaction::transfer(from, to, 1000, 5);
        let compressed = tx.compress();
        let decompressed = L2Transaction::decompress(&compressed).unwrap();

        assert_eq!(tx.from, decompressed.from);
        assert_eq!(tx.to, decompressed.to);
        assert_eq!(tx.value, decompressed.value);
        assert_eq!(tx.nonce, decompressed.nonce);
    }

    #[test]
    fn test_gas_estimation() {
        let from = Address::from_hex("0x1234567890123456789012345678901234567890").unwrap();
        let to = Address::from_hex("0xabcdefabcdefabcdefabcdefabcdefabcdefabcd").unwrap();

        // Simple transfer
        let tx1 = L2Transaction::transfer(from, to, 1000, 0);
        assert_eq!(tx1.estimate_gas(), 21000);

        // With data
        let tx2 = L2Transaction::new(from, Some(to), 0, vec![0, 0, 1, 1], 0);
        assert_eq!(tx2.estimate_gas(), 21000 + 4 + 4 + 16 + 16);
    }
}
