/// DJB2-variant hash used by BO3's asset pipeline. Case-insensitive, seeded at
/// 5381. 0 is reserved as a sentinel for "invalid", so anything that hashes to
/// 0 is bumped to 1.
pub fn hash(name: &str) -> u32 {
    let mut num: u32 = 5381;
    for c in name.chars() {
        let lc = c.to_ascii_lowercase() as u32;
        num = lc
            .wrapping_add(num << 6)
            .wrapping_add(num << 16)
            .wrapping_sub(num);
    }
    if num == 0 { 1 } else { num }
}
