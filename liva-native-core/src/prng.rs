#![allow(dead_code)]

pub struct Mulberry32 {
    seed: u32,
}

impl Mulberry32 {
    pub fn new(seed_str: &str) -> Self {
        let mut h = 0xdead_beef_u32;
        for val in seed_str.encode_utf16() {
            h = (h ^ (val as u32)).wrapping_mul(2654435761);
        }
        let seed = h ^ (h >> 16);
        Self { seed }
    }

    #[allow(dead_code)]
    pub fn from_seed_u32(seed: u32) -> Self {
        Self { seed }
    }

    pub fn next_f64(&mut self) -> f64 {
        self.seed = self.seed.wrapping_add(0x6D2B79F5);
        let mut t = self.seed;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        let term = (t ^ (t >> 7)).wrapping_mul(t | 61);
        t = t ^ t.wrapping_add(term);
        let final_val = (t ^ (t >> 14)) as f64;
        final_val / 4294967296.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mulberry32_matches_js_bit_for_bit() {
        let mut rng = Mulberry32::new("test");

        let val1 = rng.next_f64();
        let val2 = rng.next_f64();
        let val3 = rng.next_f64();

        // Check against the JS reference outputs:
        // 0.3707022285088897
        // 0.7425355203449726
        // 0.6915797765832394
        assert!((val1 - 0.3707022285088897).abs() < 1e-15);
        assert!((val2 - 0.7425355203449726).abs() < 1e-15);
        assert!((val3 - 0.6915797765832394).abs() < 1e-15);
    }

    #[test]
    fn test_mulberry32_matches_js_with_emoji() {
        let mut rng = Mulberry32::new("test😊");

        let val1 = rng.next_f64();
        let val2 = rng.next_f64();
        let val3 = rng.next_f64();

        // Check against the JS reference outputs computed using node:
        // 0.6914067640900612
        // 0.2145222551189363
        // 0.5337650158908218
        assert!((val1 - 0.6914067640900612).abs() < 1e-15);
        assert!((val2 - 0.2145222551189363).abs() < 1e-15);
        assert!((val3 - 0.5337650158908218).abs() < 1e-15);
    }
}
