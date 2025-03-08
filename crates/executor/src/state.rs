use std::{
    fs::File,
    io::{Seek, Write},
};

use hashbrown::{
    hash_map::{DefaultHashBuilder, Entry, OccupiedEntry, VacantEntry},
    HashMap,
};
use nohash_hasher::BuildNoHashHasher;
use serde::{Deserialize, Serialize};

use crate::{events::MemoryRecord, syscalls::SyscallCode, ExecutorMode};

type MemoryHasher = DefaultHashBuilder;

/// Is memory
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct Memory {
    /// TODO: docs
    pub registers: [Option<MemoryRecord>; 32],
    memory: HashMap<u32, MemoryRecord, MemoryHasher>,
}

/// TODO: docs
pub enum MemEntry<'a> {
    /// TODO: docs
    Occupied(MemOccupied<'a>),
    /// TODO: docs
    Vacant(MemVacant<'a>),
}

/// TODO: docs
pub enum MemVacant<'a> {
    /// TODO: docs
    Register(&'a mut Option<MemoryRecord>),
    /// TODO: docs
    HashMap(VacantEntry<'a, u32, MemoryRecord, MemoryHasher>),
}

/// TODO: docs
pub enum MemOccupied<'a> {
    /// TODO: docs
    Register(&'a mut MemoryRecord),
    /// TODO: docs
    HashMap(OccupiedEntry<'a, u32, MemoryRecord, MemoryHasher>),
}

impl<'a> MemOccupied<'a> {
    /// docs
    pub fn get(&self) -> &MemoryRecord {
        match self {
            Self::HashMap(entry) => entry.get(),
            Self::Register(record) => &*record,
        }
    }

    /// docs
    pub fn into_mut(self) -> &'a mut MemoryRecord {
        match self {
            Self::HashMap(entry) => entry.into_mut(),
            Self::Register(record) => record,
        }
    }
}

impl<'a> MemVacant<'a> {
    /// docs
    pub fn insert(self, record: MemoryRecord) -> &'a mut MemoryRecord {
        match self {
            Self::Register(value) => value.insert(record),
            Self::HashMap(entry) => entry.insert(record),
        }
    }
}

impl Memory {
    fn new() -> Self {
        let memory = HashMap::with_capacity_and_hasher(200_000, MemoryHasher::new());
        Self {
            memory,
            registers: [None; 32],
        }
    }

    #[inline(always)]
    fn translate_addr(addr: u32) -> u32 {
        // return addr;
        assert!(addr >= 0x10000);
        (addr - 0x10000) >> 2
    }

    /// inner
    pub fn into_inner(self) -> HashMap<u32, MemoryRecord, MemoryHasher> {
        self.memory
    }

    /// Gets
    pub fn get(&self, addr: &u32) -> Option<&MemoryRecord> {
        if *addr < 32 {
            return self.registers[*addr as usize].as_ref();
        }
        self.memory.get(&Self::translate_addr(*addr))
    }

    /// entry
    pub fn entry(&mut self, addr: u32) -> MemEntry<'_> {
        if addr < 32 {
            if self.registers[addr as usize].is_some() {
                return MemEntry::Occupied(MemOccupied::Register(
                    self.registers[addr as usize].as_mut().unwrap(),
                ));
            }
            return MemEntry::Vacant(MemVacant::Register(&mut self.registers[addr as usize]));
        }
        match self.memory.entry(Self::translate_addr(addr)) {
            Entry::Vacant(inner) => MemEntry::Vacant(MemVacant::HashMap(inner)),
            Entry::Occupied(inner) => MemEntry::Occupied(MemOccupied::HashMap(inner)),
        }
    }

    /// insert
    pub fn insert(&mut self, addr: u32, record: MemoryRecord) -> Option<MemoryRecord> {
        if addr < 32 {
            return std::mem::replace(&mut self.registers[addr as usize], Some(record));
        }
        self.memory.insert(Self::translate_addr(addr), record)
    }

    /// remove
    pub fn remove(&mut self, addr: &u32) -> Option<MemoryRecord> {
        if *addr < 32 {
            return std::mem::replace(&mut self.registers[*addr as usize], None);
        }
        self.memory.remove(&Self::translate_addr(*addr))
    }
}

/// Holds data describing the current state of a program's execution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[repr(C)]
pub struct ExecutionState {
    /// The program counter.
    pub pc: u32,

    /// The shard clock keeps track of how many shards have been executed.
    pub current_shard: u32,

    /// The memory which instructions operate over. Values contain the memory value and last shard
    /// + timestamp that each memory address was accessed.
    pub memory: Memory,

    /// The global clock keeps track of how many instructions have been executed through all shards.
    pub global_clk: u64,

    /// The clock increments by 4 (possibly more in syscalls) for each instruction that has been
    /// executed in this shard.
    pub clk: u32,

    /// Uninitialized memory addresses that have a specific value they should be initialized with.
    /// `SyscallHintRead` uses this to write hint data into uninitialized memory.
    pub uninitialized_memory: HashMap<u32, u32>,

    /// A stream of input values (global to the entire program).
    pub input_stream: Vec<Vec<u8>>,

    /// A ptr to the current position in the input stream incremented by `HINT_READ` opcode.
    pub input_stream_ptr: usize,

    /// A ptr to the current position in the proof stream, incremented after verifying a proof.
    pub proof_stream_ptr: usize,

    /// A stream of public values from the program (global to entire program).
    pub public_values_stream: Vec<u8>,

    /// A ptr to the current position in the public values stream, incremented when reading from
    /// `public_values_stream`.
    pub public_values_stream_ptr: usize,

    /// Keeps track of how many times a certain syscall has been called.
    pub syscall_counts: HashMap<SyscallCode, u64>,
}

impl ExecutionState {
    #[must_use]
    /// Create a new [`ExecutionState`].
    pub fn new(pc_start: u32) -> Self {
        Self {
            global_clk: 0,
            // Start at shard 1 since shard 0 is reserved for memory initialization.
            current_shard: 1,
            clk: 0,
            pc: pc_start,
            memory: Memory::new(),
            uninitialized_memory: HashMap::new(),
            input_stream: Vec::new(),
            input_stream_ptr: 0,
            public_values_stream: Vec::new(),
            public_values_stream_ptr: 0,
            proof_stream_ptr: 0,
            syscall_counts: HashMap::new(),
        }
    }
}

/// Holds data to track changes made to the runtime since a fork point.
#[derive(Debug, Clone, Default)]
#[allow(dead_code)]
pub struct ForkState {
    /// The `global_clk` value at the fork point.
    pub global_clk: u64,
    /// The original `clk` value at the fork point.
    pub clk: u32,
    /// The original `pc` value at the fork point.
    pub pc: u32,
    /// All memory changes since the fork point.
    pub memory_diff: HashMap<u32, Option<MemoryRecord>>,
    // /// The original memory access record at the fork point.
    // pub op_record: MemoryAccessRecord,
    // /// The original execution record at the fork point.
    // pub record: ExecutionRecord,
    /// Whether `emit_events` was enabled at the fork point.
    pub executor_mode: ExecutorMode,
}

impl ExecutionState {
    /// Save the execution state to a file.
    pub fn save(&self, file: &mut File) -> std::io::Result<()> {
        let mut writer = std::io::BufWriter::new(file);
        bincode::serialize_into(&mut writer, self).unwrap();
        writer.flush()?;
        writer.seek(std::io::SeekFrom::Start(0))?;
        Ok(())
    }
}
