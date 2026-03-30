/// Case-insensitive hash function used for display-name bucketing.
///
/// Shared between index build (merge.rs) and query-time lookups (symbol.rs).
/// Uses a simple multiplicative hash (factor 31) over lowercased ASCII bytes.
#[inline]
pub fn case_insensitive_hash(s: &str, bucket_count: u32) -> u32 {
    let mut h: u32 = 0;
    for b in s.as_bytes() {
        h = h
            .wrapping_mul(31)
            .wrapping_add(u32::from(b.to_ascii_lowercase()));
    }
    h % bucket_count
}

/// Case-sensitive hash function used for FQN bucketing.
///
/// FQNs like `com/example/Foo#bar().` are case-sensitive, so hashing without
/// lowercasing avoids unnecessary bucket collisions between e.g. `Foo` and `foo`.
#[inline]
pub fn case_sensitive_hash(s: &str, bucket_count: u32) -> u32 {
    let mut h: u32 = 0;
    for b in s.as_bytes() {
        h = h.wrapping_mul(31).wrapping_add(u32::from(*b));
    }
    h % bucket_count
}

/// Pack 3 lowercase ASCII bytes into a u32 trigram key.
///
/// Used by both index build and query-time trigram lookups.
#[inline]
pub fn trigram_key(a: u8, b: u8, c: u8) -> u32 {
    u32::from(a.to_ascii_lowercase())
        | (u32::from(b.to_ascii_lowercase()) << 8)
        | (u32::from(c.to_ascii_lowercase()) << 16)
}

/// Case-insensitive substring search without allocation.
///
/// `needle` must already be lowercased. Compares by folding haystack bytes
/// to lowercase on the fly.
#[inline]
pub fn contains_ignore_ascii_case(haystack: &str, needle_lower: &str) -> bool {
    if needle_lower.is_empty() {
        return true;
    }
    let h = haystack.as_bytes();
    let n = needle_lower.as_bytes();
    if n.len() > h.len() {
        return false;
    }
    h.windows(n.len()).any(|window| {
        window
            .iter()
            .zip(n)
            .all(|(a, b)| a.to_ascii_lowercase() == *b)
    })
}

/// Case-insensitive prefix check without allocation.
///
/// `prefix_lower` must already be lowercased.
#[inline]
pub fn starts_with_ignore_ascii_case(s: &str, prefix_lower: &str) -> bool {
    if prefix_lower.len() > s.len() {
        return false;
    }
    s.as_bytes()[..prefix_lower.len()]
        .iter()
        .zip(prefix_lower.as_bytes())
        .all(|(a, b)| a.to_ascii_lowercase() == *b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_is_deterministic() {
        assert_eq!(
            case_insensitive_hash("Service", 1024),
            case_insensitive_hash("Service", 1024)
        );
    }

    #[test]
    fn hash_is_case_insensitive() {
        assert_eq!(
            case_insensitive_hash("Service", 1024),
            case_insensitive_hash("service", 1024)
        );
        assert_eq!(
            case_insensitive_hash("FooBar", 1024),
            case_insensitive_hash("foobar", 1024)
        );
    }

    #[test]
    fn trigram_key_case_insensitive() {
        assert_eq!(trigram_key(b'A', b'B', b'C'), trigram_key(b'a', b'b', b'c'));
    }

    #[test]
    fn trigram_key_layout() {
        let key = trigram_key(b'a', b'b', b'c');
        assert_eq!(
            key,
            u32::from(b'a') | (u32::from(b'b') << 8) | (u32::from(b'c') << 16)
        );
    }

    #[test]
    fn contains_basic() {
        assert!(contains_ignore_ascii_case("FooBarBaz", "bar"));
        assert!(contains_ignore_ascii_case("foobar", "foobar"));
        assert!(!contains_ignore_ascii_case("foo", "foobar"));
        assert!(contains_ignore_ascii_case("anything", ""));
    }

    #[test]
    fn starts_with_basic() {
        assert!(starts_with_ignore_ascii_case("FooBar", "foo"));
        assert!(starts_with_ignore_ascii_case("foobar", "foobar"));
        assert!(!starts_with_ignore_ascii_case("foo", "foobar"));
        assert!(starts_with_ignore_ascii_case("anything", ""));
    }
}
