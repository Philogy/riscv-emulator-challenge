use hashbrown::{
    hash_map::{DefaultHashBuilder, Entry, OccupiedEntry, VacantEntry},
    HashMap,
};
use nohash_hasher::BuildNoHashHasher;
use serde::{Deserialize, Serialize};

type MemoryHasher = DefaultHashBuilder;

/// Is memory
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct MemoryMap<V> {
    /// TODO: docs
    pub registers: [Option<V>; 32],
    memory: HashMap<u32, V, MemoryHasher>,
}

/// TODO: docs
pub enum MemEntry<'a, V> {
    /// TODO: docs
    Occupied(MemOccupied<'a, V>),
    /// TODO: docs
    Vacant(MemVacant<'a, V>),
}

impl<'a, V> MemEntry<'a, V> {
    pub fn or_insert_with<F>(self, default: F) -> &'a mut V
    where
        F: FnOnce() -> V,
    {
        match self {
            Self::Vacant(vacant) => vacant.insert(default()),
            Self::Occupied(occupied) => occupied.into_mut(),
        }
    }

    pub fn or_insert(self, default: V) -> &'a mut V {
        self.or_insert_with(|| default)
    }

    pub fn and_modify<F>(self, f: F) -> MemEntry<'a, V>
    where
        F: FnOnce(&mut V),
    {
        match self {
            Self::Vacant(vacant) => Self::Vacant(vacant),
            Self::Occupied(mut occupied) => {
                occupied.modify(f);
                Self::Occupied(occupied)
            }
        }
    }
}

/// TODO: docs
pub enum MemVacant<'a, V> {
    /// TODO: docs
    Register(&'a mut Option<V>),
    /// TODO: docs
    HashMap(VacantEntry<'a, u32, V, MemoryHasher>),
}

/// TODO: docs
pub enum MemOccupied<'a, V> {
    /// TODO: docs
    Register(&'a mut V),
    /// TODO: docs
    HashMap(OccupiedEntry<'a, u32, V, MemoryHasher>),
}

impl<'a, V> MemOccupied<'a, V> {
    /// docs
    pub fn get(&self) -> &V {
        match self {
            Self::HashMap(entry) => entry.get(),
            Self::Register(record) => &*record,
        }
    }

    /// docs
    pub fn into_mut(self) -> &'a mut V {
        match self {
            Self::HashMap(entry) => entry.into_mut(),
            Self::Register(record) => record,
        }
    }

    pub fn modify<F>(&mut self, f: F)
    where
        F: FnOnce(&mut V),
    {
        match self {
            Self::HashMap(entry) => f(entry.get_mut()),
            Self::Register(value) => f(*value),
        }
    }
}

impl<'a, V> MemVacant<'a, V> {
    /// docs
    pub fn insert(self, value: V) -> &'a mut V {
        match self {
            Self::Register(reg_ptr) => reg_ptr.insert(value),
            Self::HashMap(entry) => entry.insert(value),
        }
    }
}

impl<V> MemoryMap<V> {
    pub fn with_capacity(capacity: usize) -> Self {
        let memory = HashMap::with_capacity_and_hasher(capacity, MemoryHasher::new());
        Self {
            memory,
            registers: [const { None }; 32],
        }
    }

    pub fn new() -> Self {
        Self::with_capacity(0)
    }

    #[inline(always)]
    fn translate_addr(addr: u32) -> u32 {
        // return addr;
        assert!(addr >= 0x10000);
        (addr - 0x10000) >> 2
    }

    /// inner
    pub fn into_inner(self) -> HashMap<u32, V, MemoryHasher> {
        self.memory
    }

    /// Gets
    pub fn get(&self, addr: &u32) -> Option<&V> {
        if *addr < 32 {
            return self.registers[*addr as usize].as_ref();
        }
        self.memory.get(&Self::translate_addr(*addr))
    }

    /// entry
    pub fn entry(&mut self, addr: u32) -> MemEntry<'_, V> {
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

    pub fn drain(&mut self) -> MemoryDrainer<'_, V> {
        MemoryDrainer::RegisterDrainer {
            map: self,
            register: 0,
        }
    }

    /// insert
    pub fn insert(&mut self, addr: u32, record: V) -> Option<V> {
        if addr < 32 {
            return std::mem::replace(&mut self.registers[addr as usize], Some(record));
        }
        self.memory.insert(Self::translate_addr(addr), record)
    }

    /// remove
    pub fn remove(&mut self, addr: &u32) -> Option<V> {
        if *addr < 32 {
            return std::mem::replace(&mut self.registers[*addr as usize], None);
        }
        self.memory.remove(&Self::translate_addr(*addr))
    }
}

pub enum MemoryDrainer<'a, V> {
    RegisterDrainer {
        map: &'a mut MemoryMap<V>,
        register: u8,
    },
    HashMapDrain(hashbrown::hash_map::Drain<'a, u32, V>),
}

impl<'a, V> Iterator for MemoryDrainer<'a, V> {
    type Item = (u32, V);

    fn next(&mut self) -> Option<Self::Item> {
        while let Self::RegisterDrainer { map, register } = self {
            // Check if we've reached the end of registers
            if *register == 32 {
                // Need to swap out self without having active borrows
                let drain_iter = std::mem::replace(
                    self,
                    // Create a temporary placeholder that will be immediately replaced
                    Self::RegisterDrainer {
                        map: unsafe { std::mem::transmute(std::ptr::null_mut::<MemoryMap<V>>()) },
                        register: 0,
                    },
                );

                // Extract the map from the old value to create the drain
                if let Self::RegisterDrainer { map, .. } = drain_iter {
                    *self = Self::HashMapDrain(map.memory.drain());
                    return self.next();
                }

                // This should never happen due to the match above
                unsafe { std::hint::unreachable_unchecked() }
            } else {
                // Handle register case
                let addr = *register as u32;
                *register += 1;

                // Try to remove the value at this address
                if let Some(value) = map.remove(&addr) {
                    return Some((addr, value));
                }
            }
        }

        match self {
            Self::HashMapDrain(drain) => drain.next(),
            Self::RegisterDrainer { .. } => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}
