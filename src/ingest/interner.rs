use rustc_hash::FxHashMap;

/// Deduplicated string table builder.
/// All strings go through `intern()` which returns a stable `u32` ID.
///
/// Uses a simple `HashMap<String, u32>` for dedup. Each unique string is
/// stored once in `vec` and once as a key in `map`. The extra key copy
/// is negligible — the interner runs once at index build time, not on
/// the query hot path.
pub struct StringInterner {
    map: FxHashMap<String, u32>,
    vec: Vec<String>,
}

impl StringInterner {
    pub fn with_capacity(cap: usize) -> Self {
        let mut map = FxHashMap::default();
        map.reserve(cap);
        Self {
            map,
            vec: Vec::with_capacity(cap),
        }
    }

    /// Intern a string, returning its ID. Returns existing ID if already interned.
    pub fn intern(&mut self, s: &str) -> u32 {
        if let Some(&id) = self.map.get(s) {
            return id;
        }
        let id = self.vec.len() as u32;
        self.map.insert(s.to_string(), id);
        self.vec.push(s.to_string());
        id
    }

    /// Consume the interner and return the string table.
    pub fn into_vec(self) -> Vec<String> {
        self.vec
    }
}
