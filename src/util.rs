pub trait U8Ext {
    fn from_bits(bits: [bool; 8]) -> Self;
    fn bits(self) -> [bool; 8];
}

impl U8Ext for u8 {
    /// view the bits of the int as an array of bools.
    ///
    /// The first element of the returned array is the highest-order bit
    ///
    /// e.g.
    /// ```
    /// assert_eq!(
    ///     7.bits(),
    ///     [false, false, false, false, false, true, true, true],
    /// );
    /// ```
    fn bits(self) -> [bool; 8] {
        let mut bits = [false; 8];
        for i in 0..8 {
            bits[7 - i] = ((self >> i) & 0x01) == 1
        }
        bits
    }

    /// Construct an integer from its bits in big-endian order
    ///
    /// The highest-order bit appears first (at index 0) in the array
    ///
    /// e.g.
    /// ```
    /// assert_eq!(
    ///     u8::from_bits([false, false, false, false, false, true, true, true]),
    ///     7
    /// );
    fn from_bits(bits: [bool; 8]) -> Self {
        bits.iter()
            .enumerate()
            .fold(0, |acc, (idx, &bit)| acc | ((bit as u8) << (7 - idx)))
    }
}

#[cfg(test)]
mod tests {
    use super::U8Ext;
    #[test]
    fn u8_to_bits() {
        assert_eq!(
            3.bits(),
            [false, false, false, false, false, false, true, true],
        );
    }

    #[test]
    fn u8_from_bits() {
        assert_eq!(
            u8::from_bits([false, false, false, false, false, false, true, true]),
            3
        );
    }

    #[test]
    fn u8_bits_round_trip() {
        for i in 0..=u8::MAX {
            assert_eq!(i, u8::from_bits(i.bits()));
        }
    }
}
