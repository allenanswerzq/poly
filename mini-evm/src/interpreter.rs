//! # EVM Interpreter
//!
//! The main execution loop that interprets bytecode.

use eth_primitives::{Address, H256, U256, keccak256};
use crate::error::{EvmError, Result};
use crate::opcode::Opcode;
use crate::stack::Stack;
use crate::memory::Memory;
use crate::storage::{Storage, StateDB};
use crate::context::{ExecutionContext, Log};

/// Maximum call depth
const MAX_CALL_DEPTH: usize = 1024;

/// Execution result
#[derive(Debug, Clone)]
pub struct ExecutionResult {
    /// Success or failure
    pub success: bool,
    /// Return data
    pub output: Vec<u8>,
    /// Gas used
    pub gas_used: u64,
    /// Gas remaining
    pub gas_remaining: u64,
    /// Emitted logs
    pub logs: Vec<Log>,
    /// Error message if failed
    pub error: Option<EvmError>,
}

impl ExecutionResult {
    pub fn success(output: Vec<u8>, gas_used: u64, gas_remaining: u64, logs: Vec<Log>) -> Self {
        ExecutionResult {
            success: true,
            output,
            gas_used,
            gas_remaining,
            logs,
            error: None,
        }
    }

    pub fn failure(error: EvmError, gas_used: u64) -> Self {
        ExecutionResult {
            success: false,
            output: Vec::new(),
            gas_used,
            gas_remaining: 0,
            logs: Vec::new(),
            error: Some(error),
        }
    }
}

/// EVM Interpreter
pub struct Interpreter {
    /// Program counter
    pc: usize,
    /// Bytecode being executed
    code: Vec<u8>,
    /// Stack
    stack: Stack,
    /// Memory
    memory: Memory,
    /// Return data from last call
    return_data: Vec<u8>,
    /// Gas remaining
    gas: u64,
    /// Gas used
    gas_used: u64,
    /// Emitted logs
    logs: Vec<Log>,
    /// Valid jump destinations
    jump_dests: Vec<bool>,
    /// Is stopped
    stopped: bool,
}

impl Interpreter {
    /// Create new interpreter for given bytecode
    pub fn new(code: Vec<u8>, gas: u64) -> Self {
        let code_len = code.len();
        let jump_dests = Self::analyze_jump_dests(&code);

        Interpreter {
            pc: 0,
            code,
            stack: Stack::new(),
            memory: Memory::new(),
            return_data: Vec::new(),
            gas,
            gas_used: 0,
            logs: Vec::new(),
            jump_dests,
            stopped: false,
        }
    }

    /// Analyze code for valid JUMPDEST locations
    fn analyze_jump_dests(code: &[u8]) -> Vec<bool> {
        let mut dests = vec![false; code.len()];
        let mut i = 0;

        while i < code.len() {
            let op = code[i];
            if op == 0x5b {
                // JUMPDEST
                dests[i] = true;
            }

            // Skip PUSH data
            if op >= 0x60 && op <= 0x7f {
                i += (op - 0x5f) as usize;
            }
            i += 1;
        }

        dests
    }

    /// Use gas (returns error if out of gas)
    fn use_gas(&mut self, amount: u64) -> Result<()> {
        if self.gas < amount {
            return Err(EvmError::OutOfGas {
                needed: amount,
                had: self.gas,
            });
        }
        self.gas -= amount;
        self.gas_used += amount;
        Ok(())
    }

    /// Read bytes from code at current PC
    fn read_bytes(&mut self, n: usize) -> U256 {
        let mut bytes = [0u8; 32];
        let start = 32 - n;

        for i in 0..n {
            if self.pc + 1 + i < self.code.len() {
                bytes[start + i] = self.code[self.pc + 1 + i];
            }
        }

        self.pc += n;
        U256::from_be_bytes(&bytes)
    }

    /// Execute until stopped
    pub fn run(&mut self, ctx: &ExecutionContext, state: &mut StateDB) -> ExecutionResult {
        while !self.stopped && self.pc < self.code.len() {
            match self.step(ctx, state) {
                Ok(Some(output)) => {
                    return ExecutionResult::success(
                        output,
                        self.gas_used,
                        self.gas,
                        std::mem::take(&mut self.logs),
                    );
                }
                Ok(None) => continue,
                Err(e) => {
                    return ExecutionResult::failure(e, self.gas_used);
                }
            }
        }

        // Normal stop
        ExecutionResult::success(
            Vec::new(),
            self.gas_used,
            self.gas,
            std::mem::take(&mut self.logs),
        )
    }

    /// Execute a single instruction
    fn step(&mut self, ctx: &ExecutionContext, state: &mut StateDB) -> Result<Option<Vec<u8>>> {
        if self.pc >= self.code.len() {
            self.stopped = true;
            return Ok(None);
        }

        let op_byte = self.code[self.pc];
        let op = Opcode::from_byte(op_byte)?;

        // Deduct base gas cost
        self.use_gas(op.gas_cost())?;

        match op {
            // 0x00: Stop
            Opcode::Stop => {
                self.stopped = true;
                return Ok(Some(Vec::new()));
            }

            // Arithmetic
            Opcode::Add => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a + b)?;
            }
            Opcode::Mul => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a * b)?;
            }
            Opcode::Sub => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a - b)?;
            }
            Opcode::Div => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a / b)?; // Returns 0 for div by zero
            }
            Opcode::SDiv => {
                // Signed division (simplified)
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a / b)?;
            }
            Opcode::Mod => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a % b)?;
            }
            Opcode::SMod => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a % b)?;
            }
            Opcode::AddMod => {
                let (a, b, n) = self.stack.pop3()?;
                if n.is_zero() {
                    self.stack.push(U256::ZERO)?;
                } else {
                    // (a + b) % n with 512-bit intermediate
                    // Simplified: might overflow for large values
                    self.stack.push((a + b) % n)?;
                }
            }
            Opcode::MulMod => {
                let (a, b, n) = self.stack.pop3()?;
                if n.is_zero() {
                    self.stack.push(U256::ZERO)?;
                } else {
                    self.stack.push((a * b) % n)?;
                }
            }
            Opcode::Exp => {
                let (base, exp) = self.stack.pop2()?;
                // Additional gas: 50 per byte of exponent
                let exp_bytes = (exp.bits() + 7) / 8;
                self.use_gas(50 * exp_bytes as u64)?;

                // Compute base^exp mod 2^256
                let mut result = U256::ONE;
                let mut b = base;
                let mut e = exp;

                while !e.is_zero() {
                    if (e.0[0] & 1) == 1 {
                        result = result * b;
                    }
                    e = e >> 1;
                    b = b * b;
                }
                self.stack.push(result)?;
            }
            Opcode::SignExtend => {
                let (b, x) = self.stack.pop2()?;
                // Sign extend from byte b
                if b.0[0] < 32 {
                    let bit = (b.0[0] as usize) * 8 + 7;
                    let x_bytes = x.to_be_bytes();
                    let sign_byte = x_bytes[31 - b.0[0] as usize];
                    let sign_bit = (sign_byte >> 7) & 1;

                    if sign_bit == 1 {
                        // Extend with 1s
                        let mut result = x;
                        for i in (bit + 1)..256 {
                            let limb = i / 64;
                            let bit_pos = i % 64;
                            result.0[limb] |= 1u64 << bit_pos;
                        }
                        self.stack.push(result)?;
                    } else {
                        self.stack.push(x)?;
                    }
                } else {
                    self.stack.push(x)?;
                }
            }

            // Comparison
            Opcode::Lt => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(if a < b { U256::ONE } else { U256::ZERO })?;
            }
            Opcode::Gt => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(if a > b { U256::ONE } else { U256::ZERO })?;
            }
            Opcode::SLt => {
                let (a, b) = self.stack.pop2()?;
                // Signed comparison (simplified)
                self.stack.push(if a < b { U256::ONE } else { U256::ZERO })?;
            }
            Opcode::SGt => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(if a > b { U256::ONE } else { U256::ZERO })?;
            }
            Opcode::Eq => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(if a == b { U256::ONE } else { U256::ZERO })?;
            }
            Opcode::IsZero => {
                let a = self.stack.pop()?;
                self.stack.push(if a.is_zero() { U256::ONE } else { U256::ZERO })?;
            }

            // Bitwise
            Opcode::And => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a & b)?;
            }
            Opcode::Or => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a | b)?;
            }
            Opcode::Xor => {
                let (a, b) = self.stack.pop2()?;
                self.stack.push(a ^ b)?;
            }
            Opcode::Not => {
                let a = self.stack.pop()?;
                self.stack.push(!a)?;
            }
            Opcode::Byte => {
                let (i, x) = self.stack.pop2()?;
                if i.0[0] >= 32 || i.0[1] != 0 || i.0[2] != 0 || i.0[3] != 0 {
                    self.stack.push(U256::ZERO)?;
                } else {
                    let bytes = x.to_be_bytes();
                    let byte = bytes[i.0[0] as usize];
                    self.stack.push(U256::from_u64(byte as u64))?;
                }
            }
            Opcode::Shl => {
                let (shift, value) = self.stack.pop2()?;
                if shift.0[1] != 0 || shift.0[2] != 0 || shift.0[3] != 0 || shift.0[0] >= 256 {
                    self.stack.push(U256::ZERO)?;
                } else {
                    self.stack.push(value << (shift.0[0] as u32))?;
                }
            }
            Opcode::Shr => {
                let (shift, value) = self.stack.pop2()?;
                if shift.0[1] != 0 || shift.0[2] != 0 || shift.0[3] != 0 || shift.0[0] >= 256 {
                    self.stack.push(U256::ZERO)?;
                } else {
                    self.stack.push(value >> (shift.0[0] as u32))?;
                }
            }
            Opcode::Sar => {
                let (shift, value) = self.stack.pop2()?;
                // Arithmetic right shift (simplified)
                if shift.0[0] >= 256 {
                    self.stack.push(U256::ZERO)?;
                } else {
                    self.stack.push(value >> (shift.0[0] as u32))?;
                }
            }

            // Keccak256
            Opcode::Keccak256 => {
                let (offset, size) = self.stack.pop2()?;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                // Memory expansion gas
                let mem_gas = self.memory.expand(offset + size);
                self.use_gas(mem_gas)?;

                // Dynamic gas: 6 per word
                let words = (size + 31) / 32;
                self.use_gas(6 * words as u64)?;

                let (data, _) = self.memory.slice(offset, size);
                let hash = keccak256(data);
                self.stack.push(U256::from_be_bytes(hash.as_bytes()))?;
            }

            // Environmental
            Opcode::Address => {
                let mut bytes = [0u8; 32];
                bytes[12..32].copy_from_slice(&ctx.call.address.0);
                self.stack.push(U256::from_be_bytes(&bytes))?;
            }
            Opcode::Balance => {
                let addr_val = self.stack.pop()?;
                let bytes = addr_val.to_be_bytes();
                let addr = Address::new(bytes[12..32].try_into().unwrap());
                let balance = state.balance(&addr);
                self.stack.push(balance)?;
            }
            Opcode::Origin => {
                let mut bytes = [0u8; 32];
                bytes[12..32].copy_from_slice(&ctx.tx.origin.0);
                self.stack.push(U256::from_be_bytes(&bytes))?;
            }
            Opcode::Caller => {
                let mut bytes = [0u8; 32];
                bytes[12..32].copy_from_slice(&ctx.call.caller.0);
                self.stack.push(U256::from_be_bytes(&bytes))?;
            }
            Opcode::CallValue => {
                self.stack.push(ctx.call.value)?;
            }
            Opcode::CallDataLoad => {
                let offset = self.stack.pop()?.0[0] as usize;
                let mut data = [0u8; 32];
                for i in 0..32 {
                    if offset + i < ctx.call.data.len() {
                        data[i] = ctx.call.data[offset + i];
                    }
                }
                self.stack.push(U256::from_be_bytes(&data))?;
            }
            Opcode::CallDataSize => {
                self.stack.push(U256::from_u64(ctx.call.data.len() as u64))?;
            }
            Opcode::CallDataCopy => {
                let (dest, offset, size) = self.stack.pop3()?;
                let dest = dest.0[0] as usize;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                let mem_gas = self.memory.copy_from(dest, &ctx.call.data, offset, size);
                self.use_gas(mem_gas)?;

                // Dynamic gas
                let words = (size + 31) / 32;
                self.use_gas(3 * words as u64)?;
            }
            Opcode::CodeSize => {
                self.stack.push(U256::from_u64(self.code.len() as u64))?;
            }
            Opcode::CodeCopy => {
                let (dest, offset, size) = self.stack.pop3()?;
                let dest = dest.0[0] as usize;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                let mem_gas = self.memory.copy_from(dest, &self.code, offset, size);
                self.use_gas(mem_gas)?;

                let words = (size + 31) / 32;
                self.use_gas(3 * words as u64)?;
            }
            Opcode::GasPrice => {
                self.stack.push(ctx.tx.gas_price)?;
            }
            Opcode::ExtCodeSize => {
                let addr_val = self.stack.pop()?;
                let bytes = addr_val.to_be_bytes();
                let addr = Address::new(bytes[12..32].try_into().unwrap());
                let code = state.code(&addr);
                self.stack.push(U256::from_u64(code.len() as u64))?;
            }
            Opcode::ExtCodeCopy => {
                let addr_val = self.stack.pop()?;
                let (dest, offset, size) = self.stack.pop3()?;

                let bytes = addr_val.to_be_bytes();
                let addr = Address::new(bytes[12..32].try_into().unwrap());
                let code = state.code(&addr).to_vec();

                let dest = dest.0[0] as usize;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                let mem_gas = self.memory.copy_from(dest, &code, offset, size);
                self.use_gas(mem_gas)?;
            }
            Opcode::ReturnDataSize => {
                self.stack.push(U256::from_u64(self.return_data.len() as u64))?;
            }
            Opcode::ReturnDataCopy => {
                let (dest, offset, size) = self.stack.pop3()?;
                let dest = dest.0[0] as usize;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                if offset + size > self.return_data.len() {
                    return Err(EvmError::ReturnDataOutOfBounds);
                }

                let mem_gas = self.memory.copy_from(dest, &self.return_data, offset, size);
                self.use_gas(mem_gas)?;
            }
            Opcode::ExtCodeHash => {
                let addr_val = self.stack.pop()?;
                let bytes = addr_val.to_be_bytes();
                let addr = Address::new(bytes[12..32].try_into().unwrap());
                let hash = state.code_hash(&addr);
                self.stack.push(U256::from_be_bytes(hash.as_bytes()))?;
            }

            // Block Information
            Opcode::BlockHash => {
                let block_num = self.stack.pop()?;
                // Simplified: return zero (should check if within last 256 blocks)
                self.stack.push(U256::ZERO)?;
            }
            Opcode::Coinbase => {
                let mut bytes = [0u8; 32];
                bytes[12..32].copy_from_slice(&ctx.block.coinbase.0);
                self.stack.push(U256::from_be_bytes(&bytes))?;
            }
            Opcode::Timestamp => {
                self.stack.push(U256::from_u64(ctx.block.timestamp))?;
            }
            Opcode::Number => {
                self.stack.push(U256::from_u64(ctx.block.number))?;
            }
            Opcode::PrevRandao => {
                self.stack.push(U256::from_be_bytes(ctx.block.prevrandao.as_bytes()))?;
            }
            Opcode::GasLimit => {
                self.stack.push(U256::from_u64(ctx.block.gas_limit))?;
            }
            Opcode::ChainId => {
                self.stack.push(U256::from_u64(ctx.block.chain_id))?;
            }
            Opcode::SelfBalance => {
                let balance = state.balance(&ctx.call.address);
                self.stack.push(balance)?;
            }
            Opcode::BaseFee => {
                self.stack.push(ctx.block.base_fee)?;
            }
            Opcode::BlobHash => {
                let index = self.stack.pop()?.0[0] as usize;
                if index < ctx.tx.blob_hashes.len() {
                    self.stack.push(U256::from_be_bytes(ctx.tx.blob_hashes[index].as_bytes()))?;
                } else {
                    self.stack.push(U256::ZERO)?;
                }
            }
            Opcode::BlobBaseFee => {
                self.stack.push(ctx.block.blob_base_fee)?;
            }

            // Stack Operations
            Opcode::Pop => {
                self.stack.pop()?;
            }
            Opcode::MLoad => {
                let offset = self.stack.pop()?.0[0] as usize;
                let (value, gas) = self.memory.load(offset);
                self.use_gas(gas)?;
                self.stack.push(value)?;
            }
            Opcode::MStore => {
                let (offset, value) = self.stack.pop2()?;
                let gas = self.memory.store(offset.0[0] as usize, value);
                self.use_gas(gas)?;
            }
            Opcode::MStore8 => {
                let (offset, value) = self.stack.pop2()?;
                let gas = self.memory.store8(offset.0[0] as usize, value.0[0] as u8);
                self.use_gas(gas)?;
            }
            Opcode::SLoad => {
                let key = self.stack.pop()?;
                let key_hash = H256::new(key.to_be_bytes());
                let storage = state.storage(&ctx.call.address);
                let value = storage.load(&key_hash);
                self.stack.push(value)?;
            }
            Opcode::SStore => {
                if ctx.call.is_static {
                    return Err(EvmError::WriteInStaticContext);
                }
                let (key, value) = self.stack.pop2()?;
                let key_hash = H256::new(key.to_be_bytes());
                let storage = state.storage_mut(&ctx.call.address);
                let gas = storage.store(key_hash, value);
                self.use_gas(gas)?;
            }
            Opcode::Jump => {
                let dest = self.stack.pop()?.0[0] as usize;
                if dest >= self.code.len() || !self.jump_dests[dest] {
                    return Err(EvmError::InvalidJump(dest));
                }
                self.pc = dest;
                return Ok(None);
            }
            Opcode::JumpI => {
                let (dest, cond) = self.stack.pop2()?;
                if !cond.is_zero() {
                    let dest = dest.0[0] as usize;
                    if dest >= self.code.len() || !self.jump_dests[dest] {
                        return Err(EvmError::InvalidJump(dest));
                    }
                    self.pc = dest;
                    return Ok(None);
                }
            }
            Opcode::Pc => {
                self.stack.push(U256::from_u64(self.pc as u64))?;
            }
            Opcode::MSize => {
                self.stack.push(U256::from_u64((self.memory.size_words() * 32) as u64))?;
            }
            Opcode::Gas => {
                self.stack.push(U256::from_u64(self.gas))?;
            }
            Opcode::JumpDest => {
                // No-op, just a valid jump target
            }
            Opcode::TLoad => {
                let key = self.stack.pop()?;
                let key_hash = H256::new(key.to_be_bytes());
                let storage = state.storage(&ctx.call.address);
                let value = storage.tload(&key_hash);
                self.stack.push(value)?;
            }
            Opcode::TStore => {
                if ctx.call.is_static {
                    return Err(EvmError::WriteInStaticContext);
                }
                let (key, value) = self.stack.pop2()?;
                let key_hash = H256::new(key.to_be_bytes());
                let storage = state.storage_mut(&ctx.call.address);
                storage.tstore(key_hash, value);
            }
            Opcode::MCopy => {
                let (dest, src, size) = self.stack.pop3()?;
                let gas = self.memory.copy(
                    dest.0[0] as usize,
                    src.0[0] as usize,
                    size.0[0] as usize,
                );
                self.use_gas(gas)?;
            }

            // Push operations
            Opcode::Push0 => {
                self.stack.push(U256::ZERO)?;
            }
            Opcode::Push1 => {
                let value = self.read_bytes(1);
                self.stack.push(value)?;
            }
            Opcode::Push2 => {
                let value = self.read_bytes(2);
                self.stack.push(value)?;
            }
            Opcode::Push3 => {
                let value = self.read_bytes(3);
                self.stack.push(value)?;
            }
            Opcode::Push4 => {
                let value = self.read_bytes(4);
                self.stack.push(value)?;
            }
            Opcode::Push5 => {
                let value = self.read_bytes(5);
                self.stack.push(value)?;
            }
            Opcode::Push6 => {
                let value = self.read_bytes(6);
                self.stack.push(value)?;
            }
            Opcode::Push7 => {
                let value = self.read_bytes(7);
                self.stack.push(value)?;
            }
            Opcode::Push8 => {
                let value = self.read_bytes(8);
                self.stack.push(value)?;
            }
            Opcode::Push9 => {
                let value = self.read_bytes(9);
                self.stack.push(value)?;
            }
            Opcode::Push10 => {
                let value = self.read_bytes(10);
                self.stack.push(value)?;
            }
            Opcode::Push11 => {
                let value = self.read_bytes(11);
                self.stack.push(value)?;
            }
            Opcode::Push12 => {
                let value = self.read_bytes(12);
                self.stack.push(value)?;
            }
            Opcode::Push13 => {
                let value = self.read_bytes(13);
                self.stack.push(value)?;
            }
            Opcode::Push14 => {
                let value = self.read_bytes(14);
                self.stack.push(value)?;
            }
            Opcode::Push15 => {
                let value = self.read_bytes(15);
                self.stack.push(value)?;
            }
            Opcode::Push16 => {
                let value = self.read_bytes(16);
                self.stack.push(value)?;
            }
            Opcode::Push17 => {
                let value = self.read_bytes(17);
                self.stack.push(value)?;
            }
            Opcode::Push18 => {
                let value = self.read_bytes(18);
                self.stack.push(value)?;
            }
            Opcode::Push19 => {
                let value = self.read_bytes(19);
                self.stack.push(value)?;
            }
            Opcode::Push20 => {
                let value = self.read_bytes(20);
                self.stack.push(value)?;
            }
            Opcode::Push21 => {
                let value = self.read_bytes(21);
                self.stack.push(value)?;
            }
            Opcode::Push22 => {
                let value = self.read_bytes(22);
                self.stack.push(value)?;
            }
            Opcode::Push23 => {
                let value = self.read_bytes(23);
                self.stack.push(value)?;
            }
            Opcode::Push24 => {
                let value = self.read_bytes(24);
                self.stack.push(value)?;
            }
            Opcode::Push25 => {
                let value = self.read_bytes(25);
                self.stack.push(value)?;
            }
            Opcode::Push26 => {
                let value = self.read_bytes(26);
                self.stack.push(value)?;
            }
            Opcode::Push27 => {
                let value = self.read_bytes(27);
                self.stack.push(value)?;
            }
            Opcode::Push28 => {
                let value = self.read_bytes(28);
                self.stack.push(value)?;
            }
            Opcode::Push29 => {
                let value = self.read_bytes(29);
                self.stack.push(value)?;
            }
            Opcode::Push30 => {
                let value = self.read_bytes(30);
                self.stack.push(value)?;
            }
            Opcode::Push31 => {
                let value = self.read_bytes(31);
                self.stack.push(value)?;
            }
            Opcode::Push32 => {
                let value = self.read_bytes(32);
                self.stack.push(value)?;
            }

            // Dup operations
            Opcode::Dup1 => self.stack.dup(1)?,
            Opcode::Dup2 => self.stack.dup(2)?,
            Opcode::Dup3 => self.stack.dup(3)?,
            Opcode::Dup4 => self.stack.dup(4)?,
            Opcode::Dup5 => self.stack.dup(5)?,
            Opcode::Dup6 => self.stack.dup(6)?,
            Opcode::Dup7 => self.stack.dup(7)?,
            Opcode::Dup8 => self.stack.dup(8)?,
            Opcode::Dup9 => self.stack.dup(9)?,
            Opcode::Dup10 => self.stack.dup(10)?,
            Opcode::Dup11 => self.stack.dup(11)?,
            Opcode::Dup12 => self.stack.dup(12)?,
            Opcode::Dup13 => self.stack.dup(13)?,
            Opcode::Dup14 => self.stack.dup(14)?,
            Opcode::Dup15 => self.stack.dup(15)?,
            Opcode::Dup16 => self.stack.dup(16)?,

            // Swap operations
            Opcode::Swap1 => self.stack.swap(1)?,
            Opcode::Swap2 => self.stack.swap(2)?,
            Opcode::Swap3 => self.stack.swap(3)?,
            Opcode::Swap4 => self.stack.swap(4)?,
            Opcode::Swap5 => self.stack.swap(5)?,
            Opcode::Swap6 => self.stack.swap(6)?,
            Opcode::Swap7 => self.stack.swap(7)?,
            Opcode::Swap8 => self.stack.swap(8)?,
            Opcode::Swap9 => self.stack.swap(9)?,
            Opcode::Swap10 => self.stack.swap(10)?,
            Opcode::Swap11 => self.stack.swap(11)?,
            Opcode::Swap12 => self.stack.swap(12)?,
            Opcode::Swap13 => self.stack.swap(13)?,
            Opcode::Swap14 => self.stack.swap(14)?,
            Opcode::Swap15 => self.stack.swap(15)?,
            Opcode::Swap16 => self.stack.swap(16)?,

            // Log operations
            Opcode::Log0 | Opcode::Log1 | Opcode::Log2 | Opcode::Log3 | Opcode::Log4 => {
                if ctx.call.is_static {
                    return Err(EvmError::WriteInStaticContext);
                }

                let topic_count = match op {
                    Opcode::Log0 => 0,
                    Opcode::Log1 => 1,
                    Opcode::Log2 => 2,
                    Opcode::Log3 => 3,
                    Opcode::Log4 => 4,
                    _ => unreachable!(),
                };

                let (offset, size) = self.stack.pop2()?;
                let mut topics = Vec::new();
                for _ in 0..topic_count {
                    let topic = self.stack.pop()?;
                    topics.push(H256::new(topic.to_be_bytes()));
                }

                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;
                let (data, gas) = self.memory.slice(offset, size);
                let data = data.to_vec(); // Clone to release borrow
                self.use_gas(gas)?;

                // Dynamic gas: 8 per byte
                self.use_gas(8 * size as u64)?;

                self.logs.push(Log::new(ctx.call.address, topics, data));
            }

            // Return and Revert
            Opcode::Return => {
                let (offset, size) = self.stack.pop2()?;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                let (data, gas) = self.memory.slice(offset, size);
                let data = data.to_vec(); // Clone to release borrow
                self.use_gas(gas)?;

                return Ok(Some(data));
            }
            Opcode::Revert => {
                let (offset, size) = self.stack.pop2()?;
                let offset = offset.0[0] as usize;
                let size = size.0[0] as usize;

                let (data, _) = self.memory.slice(offset, size);
                return Err(EvmError::Revert(hex::encode(data)));
            }
            Opcode::Invalid => {
                return Err(EvmError::InvalidOpcode(0xfe));
            }

            // Simplified: Not fully implemented
            Opcode::Create | Opcode::Create2 | Opcode::Call | Opcode::CallCode
            | Opcode::DelegateCall | Opcode::StaticCall | Opcode::SelfDestruct => {
                // Would need full call frame implementation
                return Err(EvmError::InvalidOpcode(op_byte));
            }
        }

        self.pc += 1;
        Ok(None)
    }

    /// Get current stack state
    pub fn stack(&self) -> &Stack {
        &self.stack
    }

    /// Get current memory state
    pub fn memory(&self) -> &Memory {
        &self.memory
    }
}

/// Helper to execute bytecode
pub fn execute(code: &[u8], ctx: &ExecutionContext, state: &mut StateDB) -> ExecutionResult {
    let mut interpreter = Interpreter::new(code.to_vec(), ctx.call.gas);
    interpreter.run(ctx, state)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_add() {
        // PUSH1 1, PUSH1 2, ADD, STOP
        // 0x60 0x01 0x60 0x02 0x01 0x00
        let code = vec![0x60, 0x01, 0x60, 0x02, 0x01, 0x00];
        let ctx = ExecutionContext::default();
        let mut state = StateDB::new();

        let result = execute(&code, &ctx, &mut state);
        assert!(result.success);
    }

    #[test]
    fn test_push_pop() {
        // PUSH1 42, POP, PUSH1 100, STOP
        let code = vec![0x60, 0x2a, 0x50, 0x60, 0x64, 0x00];
        let ctx = ExecutionContext::default();
        let mut state = StateDB::new();

        let result = execute(&code, &ctx, &mut state);
        assert!(result.success);
    }

    #[test]
    fn test_memory() {
        // PUSH1 0x42, PUSH1 0x00, MSTORE, PUSH1 0x20, PUSH1 0x00, RETURN
        let code = vec![
            0x60, 0x42,  // PUSH1 0x42
            0x60, 0x00,  // PUSH1 0x00
            0x52,        // MSTORE
            0x60, 0x20,  // PUSH1 0x20
            0x60, 0x00,  // PUSH1 0x00
            0xf3,        // RETURN
        ];
        let ctx = ExecutionContext::default();
        let mut state = StateDB::new();

        let result = execute(&code, &ctx, &mut state);
        assert!(result.success);
        assert_eq!(result.output.len(), 32);
        assert_eq!(result.output[31], 0x42);
    }

    #[test]
    fn test_jump() {
        // PUSH1 0x05, JUMP, INVALID, JUMPDEST, PUSH1 0x01, STOP
        let code = vec![
            0x60, 0x05,  // PUSH1 0x05
            0x56,        // JUMP
            0xfe,        // INVALID (should be skipped)
            0xfe,        // INVALID (should be skipped)
            0x5b,        // JUMPDEST (offset 5)
            0x60, 0x01,  // PUSH1 0x01
            0x00,        // STOP
        ];
        let ctx = ExecutionContext::default();
        let mut state = StateDB::new();

        let result = execute(&code, &ctx, &mut state);
        assert!(result.success);
    }

    #[test]
    fn test_calldata() {
        // CALLDATASIZE, PUSH1 0, PUSH1 0, CALLDATACOPY, STOP
        let code = vec![
            0x36,        // CALLDATASIZE
            0x00,        // STOP
        ];

        let mut ctx = ExecutionContext::default();
        ctx.call.data = vec![1, 2, 3, 4, 5];
        let mut state = StateDB::new();

        let mut interpreter = Interpreter::new(code, 1_000_000);
        interpreter.run(&ctx, &mut state);

        assert_eq!(interpreter.stack().peek().unwrap(), U256::from_u64(5));
    }

    #[test]
    fn test_storage() {
        // PUSH1 0x42, PUSH1 0x00, SSTORE, PUSH1 0x00, SLOAD, STOP
        let code = vec![
            0x60, 0x42,  // PUSH1 0x42
            0x60, 0x00,  // PUSH1 0x00 (key)
            0x55,        // SSTORE
            0x60, 0x00,  // PUSH1 0x00 (key)
            0x54,        // SLOAD
            0x00,        // STOP
        ];

        let ctx = ExecutionContext::default();
        let mut state = StateDB::new();

        let mut interpreter = Interpreter::new(code, 1_000_000);
        interpreter.run(&ctx, &mut state);

        assert_eq!(interpreter.stack().peek().unwrap(), U256::from_u64(0x42));
    }
}
