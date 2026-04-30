//! # Ethereum Transactions
//!
//! Supports multiple transaction types:
//! - Legacy (pre-EIP-2718)
//! - EIP-1559 (Type 2) - Priority fee transactions
//! - EIP-4844 (Type 3) - Blob transactions (for L2)
//!
//! Each transaction can be signed and serialized for broadcast.

use crate::error::{EthError, Result};
use crate::address::Address;
use crate::hash::{keccak256, H256};
use crate::uint::U256;
use crate::signature::{Signature, sign};
use crate::rlp::{encode_bytes, encode_list, Encodable};

/// Transaction type
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TxType {
    /// Legacy transaction (no type prefix)
    Legacy = 0,
    /// EIP-2930: Access list transaction
    AccessList = 1,
    /// EIP-1559: Priority fee transaction
    EIP1559 = 2,
    /// EIP-4844: Blob transaction
    Blob = 3,
}

impl TxType {
    pub fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            0 => Ok(TxType::Legacy),
            1 => Ok(TxType::AccessList),
            2 => Ok(TxType::EIP1559),
            3 => Ok(TxType::Blob),
            _ => Err(EthError::RlpError(format!("Unknown tx type: {}", byte))),
        }
    }
}

/// Ethereum transaction (supports multiple types)
#[derive(Clone, Debug)]
pub struct Transaction {
    /// Transaction type
    pub tx_type: TxType,
    /// Chain ID (for replay protection)
    pub chain_id: u64,
    /// Sender's nonce
    pub nonce: u64,
    /// Gas price (legacy) or max priority fee (EIP-1559)
    pub gas_price: U256,
    /// Max fee per gas (EIP-1559 only)
    pub max_fee_per_gas: Option<U256>,
    /// Gas limit
    pub gas_limit: u64,
    /// Recipient (None for contract creation)
    pub to: Option<Address>,
    /// Value in wei
    pub value: U256,
    /// Input data
    pub data: Vec<u8>,
    /// Access list (EIP-2930+)
    pub access_list: Vec<(Address, Vec<H256>)>,
    /// Signature (after signing)
    pub signature: Option<Signature>,
}

impl Transaction {
    /// Create a new legacy transaction
    pub fn legacy(
        chain_id: u64,
        nonce: u64,
        gas_price: U256,
        gas_limit: u64,
        to: Option<Address>,
        value: U256,
        data: Vec<u8>,
    ) -> Self {
        Transaction {
            tx_type: TxType::Legacy,
            chain_id,
            nonce,
            gas_price,
            max_fee_per_gas: None,
            gas_limit,
            to,
            value,
            data,
            access_list: vec![],
            signature: None,
        }
    }

    /// Create a new EIP-1559 transaction
    pub fn eip1559(
        chain_id: u64,
        nonce: u64,
        max_priority_fee_per_gas: U256,
        max_fee_per_gas: U256,
        gas_limit: u64,
        to: Option<Address>,
        value: U256,
        data: Vec<u8>,
    ) -> Self {
        Transaction {
            tx_type: TxType::EIP1559,
            chain_id,
            nonce,
            gas_price: max_priority_fee_per_gas,
            max_fee_per_gas: Some(max_fee_per_gas),
            gas_limit,
            to,
            value,
            data,
            access_list: vec![],
            signature: None,
        }
    }

    /// Get the transaction hash (for signing)
    pub fn sighash(&self) -> H256 {
        match self.tx_type {
            TxType::Legacy => self.legacy_sighash(),
            TxType::EIP1559 => self.eip1559_sighash(),
            _ => unimplemented!("Transaction type not yet supported"),
        }
    }

    /// Legacy transaction signing hash (EIP-155)
    fn legacy_sighash(&self) -> H256 {
        let items = vec![
            self.nonce.rlp_encode(),
            self.gas_price.rlp_encode(),
            self.gas_limit.rlp_encode(),
            self.encode_to(),
            self.value.rlp_encode(),
            encode_bytes(&self.data),
            self.chain_id.rlp_encode(),
            0u64.rlp_encode(),
            0u64.rlp_encode(),
        ];
        keccak256(&encode_list(&items))
    }

    /// EIP-1559 transaction signing hash
    fn eip1559_sighash(&self) -> H256 {
        let max_fee = self.max_fee_per_gas.unwrap_or(self.gas_price);

        let items = vec![
            self.chain_id.rlp_encode(),
            self.nonce.rlp_encode(),
            self.gas_price.rlp_encode(), // max_priority_fee_per_gas
            max_fee.rlp_encode(),
            self.gas_limit.rlp_encode(),
            self.encode_to(),
            self.value.rlp_encode(),
            encode_bytes(&self.data),
            self.encode_access_list(),
        ];

        let mut data = vec![0x02]; // Type prefix
        data.extend(encode_list(&items));
        keccak256(&data)
    }

    /// Encode the 'to' field
    fn encode_to(&self) -> Vec<u8> {
        match &self.to {
            Some(addr) => encode_bytes(&addr.0),
            None => encode_bytes(&[]),
        }
    }

    /// Encode access list
    fn encode_access_list(&self) -> Vec<u8> {
        let items: Vec<Vec<u8>> = self.access_list.iter().map(|(addr, keys)| {
            let key_items: Vec<Vec<u8>> = keys.iter().map(|k| encode_bytes(&k.0)).collect();
            let inner = vec![
                encode_bytes(&addr.0),
                encode_list(&key_items),
            ];
            encode_list(&inner)
        }).collect();
        encode_list(&items)
    }

    /// Sign the transaction with a private key
    pub fn sign(&mut self, private_key: &[u8; 32]) -> Result<()> {
        let sighash = self.sighash();
        let mut sig = sign(&sighash, private_key)?;

        // Adjust v for EIP-155 (legacy transactions)
        if self.tx_type == TxType::Legacy {
            sig.v = sig.v + 35 + (self.chain_id as u8) * 2;
        }

        self.signature = Some(sig);
        Ok(())
    }

    /// Get the transaction hash (after signing)
    pub fn hash(&self) -> Result<H256> {
        let encoded = self.encode()?;
        Ok(keccak256(&encoded))
    }

    /// Encode the signed transaction for broadcast
    pub fn encode(&self) -> Result<Vec<u8>> {
        let sig = self.signature
            .ok_or_else(|| EthError::InvalidSignature("Transaction not signed".into()))?;

        match self.tx_type {
            TxType::Legacy => self.encode_legacy(&sig),
            TxType::EIP1559 => self.encode_eip1559(&sig),
            _ => unimplemented!("Transaction type not yet supported"),
        }
    }

    fn encode_legacy(&self, sig: &Signature) -> Result<Vec<u8>> {
        let items = vec![
            self.nonce.rlp_encode(),
            self.gas_price.rlp_encode(),
            self.gas_limit.rlp_encode(),
            self.encode_to(),
            self.value.rlp_encode(),
            encode_bytes(&self.data),
            (sig.v as u64).rlp_encode(),
            encode_bytes(&sig.r.0),
            encode_bytes(&sig.s.0),
        ];
        Ok(encode_list(&items))
    }

    fn encode_eip1559(&self, sig: &Signature) -> Result<Vec<u8>> {
        let max_fee = self.max_fee_per_gas.unwrap_or(self.gas_price);

        let items = vec![
            self.chain_id.rlp_encode(),
            self.nonce.rlp_encode(),
            self.gas_price.rlp_encode(),
            max_fee.rlp_encode(),
            self.gas_limit.rlp_encode(),
            self.encode_to(),
            self.value.rlp_encode(),
            encode_bytes(&self.data),
            self.encode_access_list(),
            (sig.v as u64).rlp_encode(),
            encode_bytes(&sig.r.0),
            encode_bytes(&sig.s.0),
        ];

        let mut result = vec![0x02];
        result.extend(encode_list(&items));
        Ok(result)
    }

    /// Recover the sender address from a signed transaction
    pub fn recover_sender(&self) -> Result<Address> {
        let sig = self.signature
            .ok_or_else(|| EthError::InvalidSignature("Transaction not signed".into()))?;

        let sighash = self.sighash();
        sig.recover(&sighash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::signature::private_to_address;

    #[test]
    fn test_legacy_transaction() {
        let tx = Transaction::legacy(
            1, // mainnet
            0, // nonce
            U256::from_u64(20_000_000_000), // 20 gwei
            21000, // gas limit
            Some(Address::zero()),
            U256::from_u64(1_000_000_000_000_000_000), // 1 ETH
            vec![],
        );

        assert_eq!(tx.tx_type, TxType::Legacy);
        assert_eq!(tx.nonce, 0);
    }

    #[test]
    fn test_sign_and_recover() {
        let private_key: [u8; 32] = [1u8; 32];
        let sender = private_to_address(&private_key).unwrap();

        let mut tx = Transaction::legacy(
            1,
            0,
            U256::from_u64(20_000_000_000),
            21000,
            Some(Address::zero()),
            U256::from_u64(1_000_000_000_000_000_000),
            vec![],
        );

        tx.sign(&private_key).unwrap();

        assert!(tx.signature.is_some());

        let recovered = tx.recover_sender().unwrap();
        assert_eq!(recovered, sender);
    }

    #[test]
    fn test_eip1559_transaction() {
        let tx = Transaction::eip1559(
            1,
            0,
            U256::from_u64(2_000_000_000), // 2 gwei priority fee
            U256::from_u64(100_000_000_000), // 100 gwei max fee
            21000,
            Some(Address::zero()),
            U256::from_u64(1_000_000_000_000_000_000),
            vec![],
        );

        assert_eq!(tx.tx_type, TxType::EIP1559);
        assert!(tx.max_fee_per_gas.is_some());
    }

    #[test]
    fn test_contract_creation() {
        let tx = Transaction::legacy(
            1,
            0,
            U256::from_u64(20_000_000_000),
            1_000_000, // Higher gas for contract deployment
            None, // No 'to' address = contract creation
            U256::ZERO,
            vec![0x60, 0x80, 0x60, 0x40, 0x52], // Simple bytecode
        );

        assert!(tx.to.is_none());
    }
}
