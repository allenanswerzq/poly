//! # EVM Opcodes
//!
//! All 140+ EVM opcodes defined as an enum.
//! Reference: https://evm.codes

use crate::error::{EvmError, Result};

/// EVM Opcode with gas cost
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    // 0x00s: Stop and Arithmetic
    Stop,           // 0x00
    Add,            // 0x01
    Mul,            // 0x02
    Sub,            // 0x03
    Div,            // 0x04
    SDiv,           // 0x05 (signed)
    Mod,            // 0x06
    SMod,           // 0x07 (signed)
    AddMod,         // 0x08
    MulMod,         // 0x09
    Exp,            // 0x0a
    SignExtend,     // 0x0b

    // 0x10s: Comparison & Bitwise Logic
    Lt,             // 0x10
    Gt,             // 0x11
    SLt,            // 0x12 (signed)
    SGt,            // 0x13 (signed)
    Eq,             // 0x14
    IsZero,         // 0x15
    And,            // 0x16
    Or,             // 0x17
    Xor,            // 0x18
    Not,            // 0x19
    Byte,           // 0x1a
    Shl,            // 0x1b
    Shr,            // 0x1c
    Sar,            // 0x1d (signed)

    // 0x20s: Keccak256
    Keccak256,      // 0x20

    // 0x30s: Environmental Information
    Address,        // 0x30
    Balance,        // 0x31
    Origin,         // 0x32
    Caller,         // 0x33
    CallValue,      // 0x34
    CallDataLoad,   // 0x35
    CallDataSize,   // 0x36
    CallDataCopy,   // 0x37
    CodeSize,       // 0x38
    CodeCopy,       // 0x39
    GasPrice,       // 0x3a
    ExtCodeSize,    // 0x3b
    ExtCodeCopy,    // 0x3c
    ReturnDataSize, // 0x3d
    ReturnDataCopy, // 0x3e
    ExtCodeHash,    // 0x3f

    // 0x40s: Block Information
    BlockHash,      // 0x40
    Coinbase,       // 0x41
    Timestamp,      // 0x42
    Number,         // 0x43
    PrevRandao,     // 0x44 (was Difficulty)
    GasLimit,       // 0x45
    ChainId,        // 0x46
    SelfBalance,    // 0x47
    BaseFee,        // 0x48
    BlobHash,       // 0x49
    BlobBaseFee,    // 0x4a

    // 0x50s: Stack, Memory, Storage, Flow
    Pop,            // 0x50
    MLoad,          // 0x51
    MStore,         // 0x52
    MStore8,        // 0x53
    SLoad,          // 0x54
    SStore,         // 0x55
    Jump,           // 0x56
    JumpI,          // 0x57
    Pc,             // 0x58
    MSize,          // 0x59
    Gas,            // 0x5a
    JumpDest,       // 0x5b
    TLoad,          // 0x5c (transient)
    TStore,         // 0x5d (transient)
    MCopy,          // 0x5e

    // 0x5f-0x7f: Push operations
    Push0,          // 0x5f
    Push1,          // 0x60
    Push2,          // 0x61
    Push3,          // 0x62
    Push4,          // 0x63
    Push5,          // 0x64
    Push6,          // 0x65
    Push7,          // 0x66
    Push8,          // 0x67
    Push9,          // 0x68
    Push10,         // 0x69
    Push11,         // 0x6a
    Push12,         // 0x6b
    Push13,         // 0x6c
    Push14,         // 0x6d
    Push15,         // 0x6e
    Push16,         // 0x6f
    Push17,         // 0x70
    Push18,         // 0x71
    Push19,         // 0x72
    Push20,         // 0x73
    Push21,         // 0x74
    Push22,         // 0x75
    Push23,         // 0x76
    Push24,         // 0x77
    Push25,         // 0x78
    Push26,         // 0x79
    Push27,         // 0x7a
    Push28,         // 0x7b
    Push29,         // 0x7c
    Push30,         // 0x7d
    Push31,         // 0x7e
    Push32,         // 0x7f

    // 0x80-0x8f: Dup operations
    Dup1,           // 0x80
    Dup2,           // 0x81
    Dup3,           // 0x82
    Dup4,           // 0x83
    Dup5,           // 0x84
    Dup6,           // 0x85
    Dup7,           // 0x86
    Dup8,           // 0x87
    Dup9,           // 0x88
    Dup10,          // 0x89
    Dup11,          // 0x8a
    Dup12,          // 0x8b
    Dup13,          // 0x8c
    Dup14,          // 0x8d
    Dup15,          // 0x8e
    Dup16,          // 0x8f

    // 0x90-0x9f: Swap operations
    Swap1,          // 0x90
    Swap2,          // 0x91
    Swap3,          // 0x92
    Swap4,          // 0x93
    Swap5,          // 0x94
    Swap6,          // 0x95
    Swap7,          // 0x96
    Swap8,          // 0x97
    Swap9,          // 0x98
    Swap10,         // 0x99
    Swap11,         // 0x9a
    Swap12,         // 0x9b
    Swap13,         // 0x9c
    Swap14,         // 0x9d
    Swap15,         // 0x9e
    Swap16,         // 0x9f

    // 0xa0-0xa4: Log operations
    Log0,           // 0xa0
    Log1,           // 0xa1
    Log2,           // 0xa2
    Log3,           // 0xa3
    Log4,           // 0xa4

    // 0xf0s: System operations
    Create,         // 0xf0
    Call,           // 0xf1
    CallCode,       // 0xf2
    Return,         // 0xf3
    DelegateCall,   // 0xf4
    Create2,        // 0xf5
    StaticCall,     // 0xfa
    Revert,         // 0xfd
    Invalid,        // 0xfe
    SelfDestruct,   // 0xff
}

impl Opcode {
    /// Parse opcode from byte
    pub fn from_byte(byte: u8) -> Result<Self> {
        match byte {
            0x00 => Ok(Opcode::Stop),
            0x01 => Ok(Opcode::Add),
            0x02 => Ok(Opcode::Mul),
            0x03 => Ok(Opcode::Sub),
            0x04 => Ok(Opcode::Div),
            0x05 => Ok(Opcode::SDiv),
            0x06 => Ok(Opcode::Mod),
            0x07 => Ok(Opcode::SMod),
            0x08 => Ok(Opcode::AddMod),
            0x09 => Ok(Opcode::MulMod),
            0x0a => Ok(Opcode::Exp),
            0x0b => Ok(Opcode::SignExtend),

            0x10 => Ok(Opcode::Lt),
            0x11 => Ok(Opcode::Gt),
            0x12 => Ok(Opcode::SLt),
            0x13 => Ok(Opcode::SGt),
            0x14 => Ok(Opcode::Eq),
            0x15 => Ok(Opcode::IsZero),
            0x16 => Ok(Opcode::And),
            0x17 => Ok(Opcode::Or),
            0x18 => Ok(Opcode::Xor),
            0x19 => Ok(Opcode::Not),
            0x1a => Ok(Opcode::Byte),
            0x1b => Ok(Opcode::Shl),
            0x1c => Ok(Opcode::Shr),
            0x1d => Ok(Opcode::Sar),

            0x20 => Ok(Opcode::Keccak256),

            0x30 => Ok(Opcode::Address),
            0x31 => Ok(Opcode::Balance),
            0x32 => Ok(Opcode::Origin),
            0x33 => Ok(Opcode::Caller),
            0x34 => Ok(Opcode::CallValue),
            0x35 => Ok(Opcode::CallDataLoad),
            0x36 => Ok(Opcode::CallDataSize),
            0x37 => Ok(Opcode::CallDataCopy),
            0x38 => Ok(Opcode::CodeSize),
            0x39 => Ok(Opcode::CodeCopy),
            0x3a => Ok(Opcode::GasPrice),
            0x3b => Ok(Opcode::ExtCodeSize),
            0x3c => Ok(Opcode::ExtCodeCopy),
            0x3d => Ok(Opcode::ReturnDataSize),
            0x3e => Ok(Opcode::ReturnDataCopy),
            0x3f => Ok(Opcode::ExtCodeHash),

            0x40 => Ok(Opcode::BlockHash),
            0x41 => Ok(Opcode::Coinbase),
            0x42 => Ok(Opcode::Timestamp),
            0x43 => Ok(Opcode::Number),
            0x44 => Ok(Opcode::PrevRandao),
            0x45 => Ok(Opcode::GasLimit),
            0x46 => Ok(Opcode::ChainId),
            0x47 => Ok(Opcode::SelfBalance),
            0x48 => Ok(Opcode::BaseFee),
            0x49 => Ok(Opcode::BlobHash),
            0x4a => Ok(Opcode::BlobBaseFee),

            0x50 => Ok(Opcode::Pop),
            0x51 => Ok(Opcode::MLoad),
            0x52 => Ok(Opcode::MStore),
            0x53 => Ok(Opcode::MStore8),
            0x54 => Ok(Opcode::SLoad),
            0x55 => Ok(Opcode::SStore),
            0x56 => Ok(Opcode::Jump),
            0x57 => Ok(Opcode::JumpI),
            0x58 => Ok(Opcode::Pc),
            0x59 => Ok(Opcode::MSize),
            0x5a => Ok(Opcode::Gas),
            0x5b => Ok(Opcode::JumpDest),
            0x5c => Ok(Opcode::TLoad),
            0x5d => Ok(Opcode::TStore),
            0x5e => Ok(Opcode::MCopy),

            0x5f => Ok(Opcode::Push0),
            0x60 => Ok(Opcode::Push1),
            0x61 => Ok(Opcode::Push2),
            0x62 => Ok(Opcode::Push3),
            0x63 => Ok(Opcode::Push4),
            0x64 => Ok(Opcode::Push5),
            0x65 => Ok(Opcode::Push6),
            0x66 => Ok(Opcode::Push7),
            0x67 => Ok(Opcode::Push8),
            0x68 => Ok(Opcode::Push9),
            0x69 => Ok(Opcode::Push10),
            0x6a => Ok(Opcode::Push11),
            0x6b => Ok(Opcode::Push12),
            0x6c => Ok(Opcode::Push13),
            0x6d => Ok(Opcode::Push14),
            0x6e => Ok(Opcode::Push15),
            0x6f => Ok(Opcode::Push16),
            0x70 => Ok(Opcode::Push17),
            0x71 => Ok(Opcode::Push18),
            0x72 => Ok(Opcode::Push19),
            0x73 => Ok(Opcode::Push20),
            0x74 => Ok(Opcode::Push21),
            0x75 => Ok(Opcode::Push22),
            0x76 => Ok(Opcode::Push23),
            0x77 => Ok(Opcode::Push24),
            0x78 => Ok(Opcode::Push25),
            0x79 => Ok(Opcode::Push26),
            0x7a => Ok(Opcode::Push27),
            0x7b => Ok(Opcode::Push28),
            0x7c => Ok(Opcode::Push29),
            0x7d => Ok(Opcode::Push30),
            0x7e => Ok(Opcode::Push31),
            0x7f => Ok(Opcode::Push32),

            0x80 => Ok(Opcode::Dup1),
            0x81 => Ok(Opcode::Dup2),
            0x82 => Ok(Opcode::Dup3),
            0x83 => Ok(Opcode::Dup4),
            0x84 => Ok(Opcode::Dup5),
            0x85 => Ok(Opcode::Dup6),
            0x86 => Ok(Opcode::Dup7),
            0x87 => Ok(Opcode::Dup8),
            0x88 => Ok(Opcode::Dup9),
            0x89 => Ok(Opcode::Dup10),
            0x8a => Ok(Opcode::Dup11),
            0x8b => Ok(Opcode::Dup12),
            0x8c => Ok(Opcode::Dup13),
            0x8d => Ok(Opcode::Dup14),
            0x8e => Ok(Opcode::Dup15),
            0x8f => Ok(Opcode::Dup16),

            0x90 => Ok(Opcode::Swap1),
            0x91 => Ok(Opcode::Swap2),
            0x92 => Ok(Opcode::Swap3),
            0x93 => Ok(Opcode::Swap4),
            0x94 => Ok(Opcode::Swap5),
            0x95 => Ok(Opcode::Swap6),
            0x96 => Ok(Opcode::Swap7),
            0x97 => Ok(Opcode::Swap8),
            0x98 => Ok(Opcode::Swap9),
            0x99 => Ok(Opcode::Swap10),
            0x9a => Ok(Opcode::Swap11),
            0x9b => Ok(Opcode::Swap12),
            0x9c => Ok(Opcode::Swap13),
            0x9d => Ok(Opcode::Swap14),
            0x9e => Ok(Opcode::Swap15),
            0x9f => Ok(Opcode::Swap16),

            0xa0 => Ok(Opcode::Log0),
            0xa1 => Ok(Opcode::Log1),
            0xa2 => Ok(Opcode::Log2),
            0xa3 => Ok(Opcode::Log3),
            0xa4 => Ok(Opcode::Log4),

            0xf0 => Ok(Opcode::Create),
            0xf1 => Ok(Opcode::Call),
            0xf2 => Ok(Opcode::CallCode),
            0xf3 => Ok(Opcode::Return),
            0xf4 => Ok(Opcode::DelegateCall),
            0xf5 => Ok(Opcode::Create2),
            0xfa => Ok(Opcode::StaticCall),
            0xfd => Ok(Opcode::Revert),
            0xfe => Ok(Opcode::Invalid),
            0xff => Ok(Opcode::SelfDestruct),

            _ => Err(EvmError::InvalidOpcode(byte)),
        }
    }

    /// Get the base gas cost for this opcode
    pub fn gas_cost(&self) -> u64 {
        match self {
            Opcode::Stop => 0,
            Opcode::Add | Opcode::Sub => 3,
            Opcode::Mul | Opcode::Div | Opcode::SDiv => 5,
            Opcode::Mod | Opcode::SMod | Opcode::SignExtend => 5,
            Opcode::AddMod | Opcode::MulMod => 8,
            Opcode::Exp => 10, // + dynamic
            Opcode::Lt | Opcode::Gt | Opcode::SLt | Opcode::SGt | Opcode::Eq => 3,
            Opcode::IsZero | Opcode::And | Opcode::Or | Opcode::Xor | Opcode::Not => 3,
            Opcode::Byte | Opcode::Shl | Opcode::Shr | Opcode::Sar => 3,
            Opcode::Keccak256 => 30, // + dynamic
            Opcode::Address | Opcode::Origin | Opcode::Caller | Opcode::CallValue => 2,
            Opcode::CallDataLoad | Opcode::CallDataSize | Opcode::CodeSize => 3,
            Opcode::CallDataCopy | Opcode::CodeCopy => 3, // + dynamic
            Opcode::GasPrice | Opcode::ReturnDataSize => 2,
            Opcode::ReturnDataCopy => 3, // + dynamic
            Opcode::ExtCodeSize | Opcode::ExtCodeHash => 100, // cold: 2600
            Opcode::ExtCodeCopy => 100, // + dynamic
            Opcode::Balance => 100, // cold: 2600
            Opcode::BlockHash => 20,
            Opcode::Coinbase | Opcode::Timestamp | Opcode::Number => 2,
            Opcode::PrevRandao | Opcode::GasLimit | Opcode::ChainId => 2,
            Opcode::SelfBalance | Opcode::BaseFee | Opcode::BlobBaseFee => 5,
            Opcode::BlobHash => 3,
            Opcode::Pop => 2,
            Opcode::MLoad | Opcode::MStore | Opcode::MStore8 => 3, // + expansion
            Opcode::SLoad => 100, // cold: 2100
            Opcode::SStore => 100, // complex: 100-20000
            Opcode::Jump => 8,
            Opcode::JumpI => 10,
            Opcode::Pc | Opcode::MSize | Opcode::Gas => 2,
            Opcode::JumpDest => 1,
            Opcode::TLoad | Opcode::TStore => 100,
            Opcode::MCopy => 3, // + dynamic
            Opcode::Push0 => 2,
            Opcode::Push1 | Opcode::Push2 | Opcode::Push3 | Opcode::Push4 |
            Opcode::Push5 | Opcode::Push6 | Opcode::Push7 | Opcode::Push8 |
            Opcode::Push9 | Opcode::Push10 | Opcode::Push11 | Opcode::Push12 |
            Opcode::Push13 | Opcode::Push14 | Opcode::Push15 | Opcode::Push16 |
            Opcode::Push17 | Opcode::Push18 | Opcode::Push19 | Opcode::Push20 |
            Opcode::Push21 | Opcode::Push22 | Opcode::Push23 | Opcode::Push24 |
            Opcode::Push25 | Opcode::Push26 | Opcode::Push27 | Opcode::Push28 |
            Opcode::Push29 | Opcode::Push30 | Opcode::Push31 | Opcode::Push32 => 3,
            Opcode::Dup1 | Opcode::Dup2 | Opcode::Dup3 | Opcode::Dup4 |
            Opcode::Dup5 | Opcode::Dup6 | Opcode::Dup7 | Opcode::Dup8 |
            Opcode::Dup9 | Opcode::Dup10 | Opcode::Dup11 | Opcode::Dup12 |
            Opcode::Dup13 | Opcode::Dup14 | Opcode::Dup15 | Opcode::Dup16 => 3,
            Opcode::Swap1 | Opcode::Swap2 | Opcode::Swap3 | Opcode::Swap4 |
            Opcode::Swap5 | Opcode::Swap6 | Opcode::Swap7 | Opcode::Swap8 |
            Opcode::Swap9 | Opcode::Swap10 | Opcode::Swap11 | Opcode::Swap12 |
            Opcode::Swap13 | Opcode::Swap14 | Opcode::Swap15 | Opcode::Swap16 => 3,
            Opcode::Log0 => 375,
            Opcode::Log1 => 750,
            Opcode::Log2 => 1125,
            Opcode::Log3 => 1500,
            Opcode::Log4 => 1875,
            Opcode::Create => 32000,
            Opcode::Call => 100, // complex
            Opcode::CallCode => 100,
            Opcode::Return => 0,
            Opcode::DelegateCall => 100,
            Opcode::Create2 => 32000,
            Opcode::StaticCall => 100,
            Opcode::Revert => 0,
            Opcode::Invalid => 0,
            Opcode::SelfDestruct => 5000,
        }
    }

    /// Get the number of bytes this opcode consumes after itself
    pub fn data_size(&self) -> usize {
        match self {
            Opcode::Push1 => 1,
            Opcode::Push2 => 2,
            Opcode::Push3 => 3,
            Opcode::Push4 => 4,
            Opcode::Push5 => 5,
            Opcode::Push6 => 6,
            Opcode::Push7 => 7,
            Opcode::Push8 => 8,
            Opcode::Push9 => 9,
            Opcode::Push10 => 10,
            Opcode::Push11 => 11,
            Opcode::Push12 => 12,
            Opcode::Push13 => 13,
            Opcode::Push14 => 14,
            Opcode::Push15 => 15,
            Opcode::Push16 => 16,
            Opcode::Push17 => 17,
            Opcode::Push18 => 18,
            Opcode::Push19 => 19,
            Opcode::Push20 => 20,
            Opcode::Push21 => 21,
            Opcode::Push22 => 22,
            Opcode::Push23 => 23,
            Opcode::Push24 => 24,
            Opcode::Push25 => 25,
            Opcode::Push26 => 26,
            Opcode::Push27 => 27,
            Opcode::Push28 => 28,
            Opcode::Push29 => 29,
            Opcode::Push30 => 30,
            Opcode::Push31 => 31,
            Opcode::Push32 => 32,
            _ => 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opcode_from_byte() {
        assert_eq!(Opcode::from_byte(0x00).unwrap(), Opcode::Stop);
        assert_eq!(Opcode::from_byte(0x01).unwrap(), Opcode::Add);
        assert_eq!(Opcode::from_byte(0x60).unwrap(), Opcode::Push1);
        assert!(Opcode::from_byte(0xef).is_err());
    }

    #[test]
    fn test_push_data_size() {
        assert_eq!(Opcode::Push1.data_size(), 1);
        assert_eq!(Opcode::Push32.data_size(), 32);
        assert_eq!(Opcode::Add.data_size(), 0);
    }
}
