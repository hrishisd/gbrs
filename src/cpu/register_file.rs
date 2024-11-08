use serde::{Deserialize, Serialize};

/// The state of the cpu registers
///
/// There are 8 8-bit registers (A, F, B, C, D, E, H, L) and two 16-bit registers (SP and PC)
/// Some instructions allow you to use the 8-bit registers as 16-bit virtual registers by pairing them up.
/// (AF, BC, DE, HL)
///
/// The A register is the accumulator register.
/// The F register is the flags register and is not directly accessible.
/// Instead, the upper 4 bits are used to store flags from the results of math operations.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct Registers {
    pub a: u8,
    pub f: u8,
    pub b: u8,
    pub c: u8,
    pub d: u8,
    pub e: u8,
    pub h: u8,
    pub l: u8,
    /// Stack pointer
    pub sp: u16,
    /// Program counter
    pub pc: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies one of the 8-bit registers
pub enum R8 {
    A,
    B,
    C,
    D,
    E,
    H,
    L,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Identifies one of the 16-bit registers
pub enum R16 {
    AF,
    BC,
    DE,
    /// Functions as a 16-bit register that can be used to point to addresses in memory.
    HL,
    SP,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flag {
    /// Set when the result of a math operation is 0, or when two values match while executing the CP instruction
    Z,
    /// Set if subtraction was performed by the last math instruction
    N,
    /// Set if a carry occured from the lower half-byte in the last math operation
    H,
    /// Set if a carry occured from the last math operation, or if register A is the smaller value while executing the CP instruction
    C,
}

impl Registers {
    pub fn create() -> Self {
        Registers {
            a: 0,
            f: 0,
            b: 0,
            c: 0,
            d: 0,
            e: 0,
            h: 0,
            l: 0,
            sp: 0xFFFE,
            pc: 0x000,
        }
    }

    /// Read the value from an 8-bit register.
    pub fn r8(&self, reg: R8) -> u8 {
        match reg {
            R8::A => self.a,
            R8::B => self.b,
            R8::C => self.c,
            R8::D => self.d,
            R8::E => self.e,
            R8::H => self.h,
            R8::L => self.l,
        }
    }

    /// Set the value of an 8-bit register.
    pub fn set_r8(&mut self, reg: R8, val: u8) {
        match reg {
            R8::A => self.a = val,
            R8::B => self.b = val,
            R8::C => self.c = val,
            R8::D => self.d = val,
            R8::E => self.e = val,
            R8::H => self.h = val,
            R8::L => self.l = val,
        }
    }

    pub fn af(&self) -> u16 {
        u16::from_be_bytes([self.a, self.f])
    }

    pub fn bc(&self) -> u16 {
        u16::from_be_bytes([self.b, self.c])
    }

    pub fn de(&self) -> u16 {
        u16::from_be_bytes([self.d, self.e])
    }

    pub fn hl(&self) -> u16 {
        u16::from_be_bytes([self.h, self.l])
    }

    pub fn r16(&self, r: R16) -> u16 {
        let (hi, lo) = match r {
            R16::AF => (self.a, self.f),
            R16::BC => (self.b, self.c),
            R16::DE => (self.d, self.e),
            R16::HL => (self.h, self.l),
            R16::SP => return self.sp,
        };
        u16::from_be_bytes([hi, lo])
    }

    pub fn set_r16(&mut self, r: R16, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        match r {
            R16::AF => {
                self.a = hi;
                self.f = lo;
            }
            R16::BC => {
                self.b = hi;
                self.c = lo;
            }
            R16::DE => {
                self.d = hi;
                self.e = lo;
            }
            R16::HL => {
                self.h = hi;
                self.l = lo;
            }
            R16::SP => self.sp = word,
        }
    }

    pub fn set_af(&mut self, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        self.a = hi;
        self.f = lo;
    }

    pub fn set_bc(&mut self, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        self.b = hi;
        self.c = lo;
    }

    pub fn set_de(&mut self, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        self.d = hi;
        self.e = lo;
    }

    pub fn set_hl(&mut self, word: u16) {
        let [hi, lo] = word.to_be_bytes();
        self.h = hi;
        self.l = lo;
    }

    pub fn flag(&self, flag: Flag) -> bool {
        let shift = match flag {
            Flag::Z => 7,
            Flag::N => 6,
            Flag::H => 5,
            Flag::C => 4,
        };
        (self.f & 1 << shift) > 0
    }

    pub fn set_flag(&mut self, flag: Flag, bit: bool) {
        let shift = match flag {
            Flag::Z => 7,
            Flag::N => 6,
            Flag::H => 5,
            Flag::C => 4,
        };
        let flag = 1 << shift;
        if bit {
            self.f |= flag;
        } else {
            self.f &= !flag;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Flag, Registers};

    #[test]
    fn test_flags() {
        use Flag::*;
        let mut regs = Registers::create();
        for flag in [Z, N, H, C] {
            assert!(!regs.flag(flag));
            regs.set_flag(flag, true);
            assert!(regs.flag(flag));
            regs.set_flag(flag, false);
            assert!(!regs.flag(flag));
        }
    }
}
