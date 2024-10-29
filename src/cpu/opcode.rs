use super::{
    register_file::{Flag, R16, R8},
    Cpu, ImeState,
};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RstVec {
    X00 = 0x00,
    X08 = 0x08,
    X10 = 0x10,
    X18 = 0x18,
    X20 = 0x20,
    X28 = 0x28,
    X30 = 0x30,
    X38 = 0x38,
}

pub enum CC {
    /// Execute if Z is set
    Z,
    /// Execute if Z is not set
    NZ,
    /// Execute if C is set
    C,
    /// Execute if C is not set
    NC,
}

/// Implementation of the unique types of cpu instructions.
///
/// Each function simulates the execution of an instruction and returns the number of T-cycles it takes. e.g. [Cpu::nop] returns 4.
impl Cpu {
    // --- utility functions ---
    /// Fetch the 8-bit immediate that follows the opcode, and advance PC.
    fn fetch_imm8(&mut self) -> u8 {
        let res = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        res
    }

    /// Fetch the 16-bit immediate that follows the opcode, and advance PC.
    fn fetch_imm16(&mut self) -> u16 {
        let res = self.mmu.read_word(self.regs.pc);
        self.regs.pc += 2;
        res
    }

    /// Pushes the word on to the stack in little-endian order (the lower-order byte is at the lower address).
    pub fn push_u16(&mut self, word: u16) {
        // println!("PUSH {:#04X} at addr {:#04X}", word, self.regs.sp);
        let [lo, hi] = word.to_le_bytes();
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.mmu.write_byte(self.regs.sp, hi);
        self.regs.sp = self.regs.sp.wrapping_sub(1);
        self.mmu.write_byte(self.regs.sp, lo);
    }

    fn pop_u16(&mut self) -> u16 {
        let lo = self.mmu.read_byte(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        let hi = self.mmu.read_byte(self.regs.sp);
        self.regs.sp = self.regs.sp.wrapping_add(1);
        // println!(
        //     "POP {:04X} at addr {:#04X}",
        //     u16::from_le_bytes([lo, hi]),
        //     self.regs.sp
        // );
        u16::from_le_bytes([lo, hi])
    }

    fn check_cond(&mut self, cond: CC) -> bool {
        use Flag::{C, Z};
        match cond {
            CC::Z => self.regs.flag(Z),
            CC::NZ => !self.regs.flag(Z),
            CC::C => self.regs.flag(C),
            CC::NC => !self.regs.flag(C),
        }
    }

    // --- 8-bit Arithmetic and Logic Instructions

    /// Add the value and carry flag to A, and set flags accordingly
    fn alu_adc(&mut self, x: u8) {
        self.alu_add(x, self.regs.flag(Flag::C));
    }

    /// Add the value and carry bit to A, and set flags accordingly
    fn alu_add(&mut self, x: u8, carry: bool) {
        use Flag::*;
        let carry = carry as u8;
        let a = self.regs.a;
        let result = a.wrapping_add(x).wrapping_add(carry);

        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs
            .set_flag(C, (a as u16 + x as u16 + carry as u16) > (u8::MAX as u16));
        self.regs
            .set_flag(H, (a & 0x0F) + (x & 0x0F) + carry > 0x0F);
        self.regs.a = result;
    }

    /// ADC A,r8
    pub fn adc_a_r8(&mut self, r: R8) -> u8 {
        self.alu_adc(self.regs.r8(r));
        4
    }

    /// ADC A,\[HL\]
    pub fn adc_a_ref_hl(&mut self) -> u8 {
        self.alu_adc(self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// ADC A,n8
    pub fn adc_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_adc(imm);
        8
    }

    /// ADD A,r8
    pub fn add_a_r8(&mut self, r: R8) -> u8 {
        self.alu_add(self.regs.r8(r), false);
        4
    }

    /// ADD A,\[HL\]
    pub fn add_a_ref_hl(&mut self) -> u8 {
        self.alu_add(self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// ADD A,n8
    pub fn add_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_add(imm, false);
        8
    }

    /// AND the value with A, and set flags
    fn alu_and(&mut self, x: u8) {
        use Flag::*;
        self.regs.a &= x;
        self.regs.set_flag(Z, self.regs.a == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, true);
        self.regs.set_flag(C, false);
    }

    /// AND A,r8
    pub fn and_a_r8(&mut self, r: R8) -> u8 {
        self.alu_and(self.regs.r8(r));
        4
    }

    /// AND A,\[HL\]
    pub fn and_a_ref_hl(&mut self) -> u8 {
        self.alu_and(self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// AND A,n8
    pub fn and_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_and(imm);
        8
    }

    /// Subtract the carry flag and y from A, set flags accordingly, and return the result
    fn alu_sub(&mut self, x: u8, carry: bool) {
        use Flag::*;
        let a = self.regs.a;
        let result = a.wrapping_sub(x).wrapping_sub(carry as u8);
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, true);
        self.regs
            .set_flag(H, (a & 0x0F) < ((x & 0x0F) + carry as u8));
        self.regs.set_flag(C, x as u16 + carry as u16 > a as u16);
        self.regs.a = result;
    }

    /// Subtract the value from A and set flags accordingly, but don't store the result. This is useful for ComParing values
    fn alu_cp(&mut self, x: u8) {
        let prev_val = self.regs.a;
        self.alu_sub(x, false);
        self.regs.a = prev_val;
    }

    /// CP A,r8
    pub fn cp_a_r8(&mut self, r: R8) -> u8 {
        self.alu_cp(self.regs.r8(r));
        4
    }

    /// CP A,\[HL\]
    pub fn cp_a_ref_hl(&mut self) -> u8 {
        self.alu_cp(self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// CP A,n8
    pub fn cp_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_cp(imm);
        8
    }

    /// Decrements the value by 1, sets flags, and returns the result
    fn alu_dec(&mut self, x: u8) -> u8 {
        use Flag::*;
        let result = x.wrapping_sub(1);
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, true);
        self.regs.set_flag(H, (x & 0x0F) == 0);
        result
    }

    /// DEC r8
    pub fn dec_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_dec(self.regs.r8(r));
        self.regs.set_r8(r, result);
        4
    }

    /// DEC \[HL\]
    pub fn dec_ref_hl(&mut self) -> u8 {
        let result = self.alu_dec(self.mmu.read_byte(self.regs.hl()));
        self.mmu.write_byte(self.regs.hl(), result);
        12
    }

    /// Increments the value, sets flags, and returns the result
    fn alu_inc(&mut self, val: u8) -> u8 {
        use Flag::*;
        let result = val.wrapping_add(1);
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, val & 0x0F == 0x0F);
        result
    }

    /// INC r8
    pub fn inc_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_inc(self.regs.r8(r));
        self.regs.set_r8(r, result);
        4
    }

    /// INC \[HL\]
    pub fn inc_ref_hl(&mut self) -> u8 {
        let result = self.alu_inc(self.mmu.read_byte(self.regs.hl()));
        self.mmu.write_byte(self.regs.hl(), result);
        12
    }

    /// ORs register A with the 8-bit value, and sets flags
    fn alu_or(&mut self, x: u8) {
        use Flag::*;
        self.regs.a |= x;
        self.regs.set_flag(Z, self.regs.a == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, false);
    }

    /// OR A,r8
    pub fn or_a_r8(&mut self, r: R8) -> u8 {
        self.alu_or(self.regs.r8(r));
        4
    }

    /// OR A,\[HL\]
    pub fn or_a_ref_hl(&mut self) -> u8 {
        self.alu_or(self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// OR A,n8
    pub fn or_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_or(imm);
        8
    }

    /// SBC A,r8
    pub fn sbc_a_r8(&mut self, r: R8) -> u8 {
        self.alu_sub(self.regs.r8(r), self.regs.flag(Flag::C));
        4
    }

    /// SBC A,\[HL\]
    pub fn sbc_a_ref_hl(&mut self) -> u8 {
        self.alu_sub(self.mmu.read_byte(self.regs.hl()), self.regs.flag(Flag::C));
        8
    }

    /// SBC A,n8
    pub fn sbc_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_sub(imm, self.regs.flag(Flag::C));
        8
    }

    /// SUB A,r8
    pub fn sub_a_r8(&mut self, r: R8) -> u8 {
        self.alu_sub(self.regs.r8(r), false);
        4
    }

    /// SUB A,\[HL\]
    pub fn sub_a_ref_hl(&mut self) -> u8 {
        self.alu_sub(self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// SUB A,n8
    pub fn sub_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_sub(imm, false);
        8
    }

    /// XORs A with the value, and sets flags
    fn alu_xor(&mut self, x: u8) {
        use Flag::*;
        self.regs.a ^= x;
        self.regs.set_flag(Z, self.regs.a == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, false);
    }

    /// XOR A,r8
    pub fn xor_a_r8(&mut self, r: R8) -> u8 {
        self.alu_xor(self.regs.r8(r));
        4
    }

    /// XOR A,\[HL\]
    pub fn xor_a_ref_hl(&mut self) -> u8 {
        self.alu_xor(self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// XOR A,n8
    pub fn xor_a_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.alu_xor(imm);
        8
    }

    // --- 16-bit Arithmetic Instructions ---

    /// ADD HL,r16
    pub fn add_hl_r16(&mut self, reg: R16) -> u8 {
        use Flag::*;
        let hl = self.regs.hl();
        let val = self.regs.r16(reg);
        let result = hl.wrapping_add(val);

        self.regs.set_flag(N, false);
        self.regs.set_flag(C, hl > 0xFFFF - val);
        // set half-carry if overflow from bit 11
        let mask = 0x0FFF;
        self.regs.set_flag(H, (hl & mask) + (val & mask) > mask);

        self.regs.set_hl(result);
        8
    }

    /// DEC r16
    pub fn dec_r16(&mut self, reg: R16) -> u8 {
        self.regs.set_r16(reg, self.regs.r16(reg).wrapping_sub(1));
        8
    }

    /// INC r16
    pub fn inc_r16(&mut self, reg: R16) -> u8 {
        self.regs.set_r16(reg, self.regs.r16(reg).wrapping_add(1));
        8
    }

    // --- Bit Operations Instructions ---

    /// Test bit u3 in register r8, set the zero flag if bit not set.
    fn test_bit_u3(&mut self, u3: u8, val: u8) {
        use Flag::*;
        let bit = (val >> u3) & 1;
        self.regs.set_flag(Z, bit == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, true);
    }

    /// BIT u3,r8
    pub fn bit_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        self.test_bit_u3(u3, self.regs.r8(reg));
        8
    }

    /// BIT u3,\[HL\]
    pub fn bit_u3_ref_hl(&mut self, u3: u8) -> u8 {
        self.test_bit_u3(u3, self.mmu.read_byte(self.regs.hl()));
        12
    }

    /// RES u3,r8
    ///
    /// Set bit u3 in register r8 to 0. Bit 0 is the rightmost one, bit 7 the leftmost one.
    pub fn res_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        let mask = !(1 << u3);
        self.regs.set_r8(reg, self.regs.r8(reg) & mask);
        8
    }

    /// RES u3,\[HL\]
    ///
    /// Set bit u3 in the byte pointed by HL to 0. Bit 0 is the rightmost one, bit 7 the leftmost one.
    pub fn res_u3_ref_hl(&mut self, u3: u8) -> u8 {
        let mask = !(1 << u3);
        let val = self.mmu.read_byte(self.regs.hl()) & mask;
        self.mmu.write_byte(self.regs.hl(), val);
        16
    }

    /// SET u3,r8
    ///
    /// Set bit u3 in register r8 to 1. Bit 0 is the rightmost one, bit 7 the leftmost one.
    pub fn set_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        let mask = 1 << u3;
        self.regs.set_r8(reg, self.regs.r8(reg) | mask);
        8
    }

    /// SET u3,\[HL\]
    ///
    /// Set bit u3 in the byte pointed by HL to 1. Bit 0 is the rightmost one, bit 7 the leftmost one.
    pub fn set_u3_ref_hl(&mut self, u3: u8) -> u8 {
        let mask = 1 << u3;
        let val = self.mmu.read_byte(self.regs.hl()) | mask;
        self.mmu.write_byte(self.regs.hl(), val);
        16
    }

    /// Swap the upper 4 bits of the byte and the lower 4 ones. Set flags accordingly.
    fn swap_byte(&mut self, val: u8) -> u8 {
        use Flag::*;
        let lower = val & 0xF;
        let upper = val & 0xF0;
        let res = (lower << 4) | (upper >> 4);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, false);
        res
    }

    /// SWAP r8
    pub fn swap_r8(&mut self, reg: R8) -> u8 {
        let val = self.swap_byte(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// SWAP \[HL\]
    pub fn swap_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let swapped = self.swap_byte(val);
        self.mmu.write_byte(self.regs.hl(), swapped);
        16
    }

    // --- Bit Shift Instructions ---

    /// Rotate bits left, through the carry flag, setting flags appropriately.
    ///
    /// ```text
    ///   ┏━ Flags ━┓ ┏━━━━━━━ u8 ━━━━━━┓
    /// ┌─╂─   C   ←╂─╂─ b7 ← ... ← b0 ←╂─┐
    /// │ ┗━━━━━━━━━┛ ┗━━━━━━━━━━━━━━━━━┛ │
    /// └─────────────────────────────────┘
    /// ```
    fn alu_rl(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = (val << 1) | (self.regs.flag(C) as u8);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x80 != 0);
        res
    }

    /// RL r8
    pub fn rl_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_rl(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// RL \[HL\]
    pub fn rl_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_rl(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// RLA
    pub fn rla(&mut self) -> u8 {
        self.rl_r8(R8::A);
        self.regs.set_flag(Flag::Z, false);
        4
    }

    /// Rotate left, setting flags appropriately
    ///
    /// ```text
    /// ┏━ Flags ━┓   ┏━━━━━━━ u8 ━━━━━━┓
    /// ┃    C   ←╂─┬─╂─ b7 ← ... ← b0 ←╂─┐
    /// ┗━━━━━━━━━┛ │ ┗━━━━━━━━━━━━━━━━━┛ │
    ///             └─────────────────────┘
    /// ```
    pub fn alu_rlc(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val.rotate_left(1);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x80 != 0);
        res
    }

    /// RLC r8
    pub fn rlc_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_rlc(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// RLC \[HL\]
    pub fn rlc_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_rlc(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// RLCA
    pub fn rlca(&mut self) -> u8 {
        self.rlc_r8(R8::A);
        self.regs.set_flag(Flag::Z, false);
        4
    }

    /// Rotate bits right, through the carry flag, setting flags appropriately.
    ///
    /// ```text
    ///   ┏━━━━━━━ u8 ━━━━━━┓ ┏━ Flags ━┓
    /// ┌─╂→ b7 → ... → b0 ─╂─╂→   C   ─╂─┐
    /// │ ┗━━━━━━━━━━━━━━━━━┛ ┗━━━━━━━━━┛ │
    /// └─────────────────────────────────┘
    /// ```
    pub fn alu_rr(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = (val >> 1) | ((self.regs.flag(C) as u8) << 7);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x01 == 1);
        res
    }

    /// RR r8
    pub fn rr_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_rr(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// RR \[HL\]
    pub fn rr_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_rr(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// RRA
    pub fn rra(&mut self) -> u8 {
        self.rr_r8(R8::A);
        self.regs.set_flag(Flag::Z, false);
        4
    }

    /// Rotate right, setting flags appropriately
    ///
    /// ```text
    ///   ┏━━━━━━━ u8 ━━━━━━┓   ┏━ Flags ━┓
    /// ┌─╂→ b7 → ... → b0 ─╂─┬─╂→   C    ┃
    /// │ ┗━━━━━━━━━━━━━━━━━┛ │ ┗━━━━━━━━━┛
    /// └─────────────────────┘
    /// ```
    pub fn alu_rrc(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val.rotate_right(1);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x01 == 1);
        res
    }

    /// RRC r8
    pub fn rrc_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_rrc(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// RRC \[HL\]
    pub fn rrc_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_rrc(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// RRCA
    pub fn rrca(&mut self) -> u8 {
        self.rrc_r8(R8::A);
        self.regs.set_flag(Flag::Z, false);
        4
    }

    /// Shift left arithmetically, setting flags appropriately
    ///
    ///```text
    /// ┏━ Flags ━┓ ┏━━━━━━━ u8 ━━━━━━┓
    /// ┃    C   ←╂─╂─ b7 ← ... ← b0 ←╂─ 0
    /// ┗━━━━━━━━━┛ ┗━━━━━━━━━━━━━━━━━┛
    /// ```
    fn alu_sla(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val << 1;
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x80 != 0);
        res
    }

    /// SLA r8
    pub fn sla_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_sla(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// SLA \[HL\]
    pub fn sla_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_sla(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// Shift right arithmetically, setting flags appropriately.
    ///
    /// `b7` remains unchanged.
    ///
    ///```text
    /// ┏━━━━━━ u8 ━━━━━━┓ ┏━ Flags ━┓
    /// ┃ b7 → ... → b0 ─╂─╂→   C    ┃
    /// ┗━━━━━━━━━━━━━━━━┛ ┗━━━━━━━━━┛
    /// ```
    fn alu_sra(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val >> 1 | (val & 0x80);
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x01 == 1);
        res
    }

    /// SRA r8
    pub fn sra_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_sra(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// SRA \[HL\]
    pub fn sra_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_sra(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    /// Shift right logically, setting flags appropriately.
    ///
    ///```text
    ///    ┏━━━━━━━ u8 ━━━━━━┓ ┏━ Flags ━┓
    /// 0 ─╂→ b7 → ... → b0 ─╂─╂→   C    ┃
    ///    ┗━━━━━━━━━━━━━━━━━┛ ┗━━━━━━━━━┛
    /// ```
    fn alu_srl(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val >> 1;
        self.regs.set_flag(Z, res == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, val & 0x01 == 1);
        res
    }

    /// SRL r8
    pub fn srl_r8(&mut self, reg: R8) -> u8 {
        let val = self.alu_srl(self.regs.r8(reg));
        self.regs.set_r8(reg, val);
        8
    }

    /// SRL \[HL\]
    pub fn srl_ref_hl(&mut self) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        let rotated = self.alu_srl(val);
        self.mmu.write_byte(self.regs.hl(), rotated);
        16
    }

    // --- Load Instructions ---

    /// LD r8,r8
    pub fn ld_r8_r8(&mut self, dest: R8, src: R8) -> u8 {
        self.regs.set_r8(dest, self.regs.r8(src));
        4
    }

    /// LD r8,n8
    pub fn ld_r8_n8(&mut self, reg: R8) -> u8 {
        let imm = self.fetch_imm8();
        self.regs.set_r8(reg, imm);
        8
    }

    /// LD r16,n16
    pub fn ld_r16_n16(&mut self, r: R16) -> u8 {
        let word = self.fetch_imm16();
        self.regs.set_r16(r, word);
        12
    }

    /// LD \[HL\],r8
    pub fn ld_ref_hl_r8(&mut self, reg: R8) -> u8 {
        self.mmu.write_byte(self.regs.hl(), self.regs.r8(reg));
        8
    }

    /// LD \[HL\],n8
    pub fn ld_ref_hl_n8(&mut self) -> u8 {
        let imm = self.fetch_imm8();
        self.mmu.write_byte(self.regs.hl(), imm);
        12
    }

    /// LD r8,\[HL\]
    pub fn ld_r8_ref_hl(&mut self, reg: R8) -> u8 {
        let val = self.mmu.read_byte(self.regs.hl());
        self.regs.set_r8(reg, val);
        8
    }

    /// LD \[r16\],A
    pub fn ld_ref_r16_a(&mut self, reg: R16) -> u8 {
        self.mmu.write_byte(self.regs.r16(reg), self.regs.a);
        8
    }

    /// LD \[n16\],A
    pub fn ld_ref_n16_a(&mut self) -> u8 {
        let addr = self.fetch_imm16();
        self.mmu.write_byte(addr, self.regs.a);
        16
    }

    /// LDH \[n16\],A
    ///
    /// Also encoded as LD \[$FF00+n8\],A
    pub fn ldh_ref_a8_a(&mut self) -> u8 {
        let offset = self.fetch_imm8();
        let addr = 0xFF00 + offset as u16;
        self.mmu.write_byte(addr, self.regs.a);
        12
    }

    /// LDH \[C\],A
    ///
    /// Also encoded as LD \[$FF00+C\], A
    pub fn ldh_ref_c_a(&mut self) -> u8 {
        let addr = 0xFF00 + (self.regs.c as u16);
        self.mmu.write_byte(addr, self.regs.a);
        8
    }

    /// LD A,\[r16\]
    pub fn ld_a_ref_r16(&mut self, reg: R16) -> u8 {
        self.regs.a = self.mmu.read_byte(self.regs.r16(reg));
        8
    }

    /// LD A,\[n16\]
    pub fn ld_a_ref_n16(&mut self) -> u8 {
        let addr = self.fetch_imm16();
        self.regs.a = self.mmu.read_byte(addr);
        16
    }

    /// LDH A,\[n16\]
    ///
    /// Also expressed as LD A,[$FF00+n8]
    pub fn ldh_a_ref_a8(&mut self) -> u8 {
        let offset = self.fetch_imm8();
        let addr = 0xFF00 + offset as u16;
        self.regs.a = self.mmu.read_byte(addr);
        12
    }

    /// LDH A,\[C\]
    ///
    /// Also expressed as LD A,[$FF00+$C]
    pub fn ldh_a_ref_c(&mut self) -> u8 {
        let addr = 0xFF00 + self.regs.c as u16;
        self.regs.a = self.mmu.read_byte(addr);
        8
    }

    /// LD \[HLI\],A
    pub fn ld_ref_hli_a(&mut self) -> u8 {
        self.ld_ref_r16_a(R16::HL);
        self.regs.set_hl(self.regs.hl().wrapping_add(1));
        8
    }

    /// LD \[HLD\],A
    pub fn ld_ref_hld_a(&mut self) -> u8 {
        self.ld_ref_r16_a(R16::HL);
        self.regs.set_hl(self.regs.hl().wrapping_sub(1));
        8
    }

    /// LD A,\[HLI\]
    pub fn ld_a_ref_hli(&mut self) -> u8 {
        self.ld_a_ref_r16(R16::HL);
        self.regs.set_hl(self.regs.hl().wrapping_add(1));
        8
    }

    /// LD A,\[HLD\]
    pub fn ld_a_ref_hld(&mut self) -> u8 {
        self.ld_a_ref_r16(R16::HL);
        self.regs.set_hl(self.regs.hl().wrapping_sub(1));
        8
    }

    // --- Jumps and Subroutines ---

    /// CALL n16
    pub fn call_n16(&mut self) -> u8 {
        let jump_addr = self.fetch_imm16();
        self.push_u16(self.regs.pc);
        self.regs.pc = jump_addr;
        24
    }

    /// CALL cc,n16
    pub fn call_cc_n16(&mut self, cc: CC) -> u8 {
        let jump_addr = self.fetch_imm16();
        if self.check_cond(cc) {
            self.push_u16(self.regs.pc);
            self.regs.pc = jump_addr;
            24
        } else {
            12
        }
    }

    /// JP HL
    pub fn jp_hl(&mut self) -> u8 {
        self.regs.pc = self.regs.hl();
        4
    }

    /// JP n16
    pub fn jp_n16(&mut self) -> u8 {
        let addr = self.fetch_imm16();
        // println!("Jumping to {addr:#X}");
        self.regs.pc = addr;
        16
    }

    /// JP cc,n16
    pub fn jp_cc_n16(&mut self, cc: CC) -> u8 {
        let addr = self.fetch_imm16();
        if self.check_cond(cc) {
            self.regs.pc = addr;
            16
        } else {
            12
        }
    }

    /// JR e8
    ///
    /// Relative Jump to address n16.
    /// The address is encoded as a signed 8-bit offset from the address immediately following the JR instruction, so the target address n16 must be between -128 and 127 bytes away.
    pub fn jr_e8(&mut self) -> u8 {
        let offset = self.fetch_imm8() as i8;
        self.regs.pc = (self.regs.pc as i16 + offset as i16) as u16;
        12
    }

    /// JR cc,n16
    pub fn jr_cc_e8(&mut self, cc: CC) -> u8 {
        let offset = self.fetch_imm8() as i8;
        if self.check_cond(cc) {
            self.regs.pc = (self.regs.pc as i16 + offset as i16) as u16;
            12
        } else {
            8
        }
    }

    /// RET
    pub fn ret(&mut self) -> u8 {
        self.regs.pc = self.pop_u16();
        16
    }

    /// RET cc
    pub fn ret_cc(&mut self, cc: CC) -> u8 {
        if self.check_cond(cc) {
            self.regs.pc = self.pop_u16();
            20
        } else {
            8
        }
    }

    /// RETI
    pub fn reti(&mut self) -> u8 {
        self.regs.pc = self.pop_u16();
        self.ime = ImeState::Enabled;
        16
    }

    /// RST vec
    pub fn rst_vec(&mut self, vec: RstVec) -> u8 {
        self.push_u16(self.regs.pc);
        self.regs.pc = vec as u16;
        16
    }

    // --- Stack Operations Instructions ---

    /// Add the signed value and SP, return the result, and set flags
    fn alu_add_sp_e8(&mut self, offset: i8) -> u16 {
        use Flag::*;
        let sp = self.regs.sp;
        let result = sp.wrapping_add(offset as i16 as u16);
        self.regs.set_flag(Z, false);
        self.regs.set_flag(N, false);

        let unsigned_offset = offset as u8;
        self.regs
            .set_flag(H, (sp & 0x0F) as u8 + (unsigned_offset & 0x0F) > 0x0F);
        self.regs
            .set_flag(C, (sp & 0xFF) + (unsigned_offset as u16 & 0xFF) > 0xFF);
        result
    }

    /// ADD SP,e8
    pub fn add_sp_e8(&mut self) -> u8 {
        let offset = self.fetch_imm8() as i8;
        self.regs.sp = self.alu_add_sp_e8(offset);
        16
    }

    /// LD [n16],SP
    pub fn ld_n16_sp(&mut self) -> u8 {
        let addr = self.fetch_imm16();
        let [lo, hi] = self.regs.sp.to_le_bytes();
        self.mmu.write_byte(addr, lo);
        self.mmu.write_byte(addr + 1, hi);
        20
    }

    /// LD HL,SP+e8
    pub fn ld_hl_sp_e8(&mut self) -> u8 {
        let offset = self.fetch_imm8() as i8;
        let word = self.alu_add_sp_e8(offset);
        self.regs.set_hl(word);
        12
    }

    /// LD SP,HL
    pub fn ld_sp_hl(&mut self) -> u8 {
        self.regs.sp = self.regs.hl();
        8
    }

    /// POP r16
    pub fn pop_r16(&mut self, reg: R16) -> u8 {
        let word = self.pop_u16();
        self.regs.set_r16(reg, word);
        if reg == R16::AF {
            self.regs.f &= 0xF0; // lower 4 bits of F are always 0
        }
        12
    }

    /// PUSH r16
    pub fn push_r16(&mut self, reg: R16) -> u8 {
        self.push_u16(self.regs.r16(reg));
        16
    }

    // --- Miscellaneous Instructions ---

    /// Complement carry flag
    pub fn ccf(&mut self) -> u8 {
        use Flag::{C, H, N};
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, !self.regs.flag(C));
        8
    }

    /// Complement accumulator
    pub fn cpl(&mut self) -> u8 {
        use Flag::{H, N};
        self.regs.a = !self.regs.a;
        self.regs.set_flag(N, true);
        self.regs.set_flag(H, true);
        8
    }

    /// Decimal adjust accumulator to get a correct BCD representation after an arithmetic instruction.
    pub fn daa(&mut self) -> u8 {
        // ref: https://ehaskins.com/2018-01-30%20Z80%20DAA/
        use Flag::*;
        let mut a = self.regs.a;
        let mut adjust = 0;
        let mut carry = self.regs.flag(C);

        if self.regs.flag(H) || (!self.regs.flag(N) && (a & 0x0F) > 9) {
            adjust |= 0x06;
        }
        if self.regs.flag(C) || (!self.regs.flag(N) && a > 0x99) {
            adjust |= 0x60;
            carry = true;
        }

        if self.regs.flag(N) {
            a = a.wrapping_sub(adjust);
        } else {
            a = a.wrapping_add(adjust);
        }

        self.regs.a = a;
        self.regs.set_flag(Z, self.regs.a == 0);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, carry);
        4
    }

    pub fn di(&mut self) -> u8 {
        self.ime = ImeState::Disabled;
        4
    }

    pub fn ei(&mut self) -> u8 {
        self.ime = ImeState::PendingEnable;
        4
    }

    pub fn halt(&mut self) -> u8 {
        // understand and implement the halt bug
        self.is_halted = true;
        4
    }

    pub fn nop(&self) -> u8 {
        4
    }

    /// Set carry flag.
    pub fn scf(&mut self) -> u8 {
        use Flag::{C, H, N};
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, true);
        4
    }

    pub fn stop(&mut self) -> u8 {
        // Stop must be followed by an additional byte that is ignored by the CPU
        self.fetch_imm8();
        panic!("STOP")
        // 4
    }
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert_eq, proptest};
    const FAKE_ROM: [u8; 0] = [];

    use crate::cpu::{
        register_file::{Flag, R8},
        Cpu, ImeState,
    };

    #[test]
    /// EI sets the IME register, but the effect is only visible after the instruction following EI is executed
    ///
    /// EI followed by DI, should not set the IME
    fn test_ime_update() {
        use ImeState::*;
        // Program is
        // EI
        // NOP
        // NOP
        let mut cpu = Cpu::new(&[0xFB, 0x00, 0x00], None, false);
        cpu.mmu.in_boot_rom = false;
        assert_eq!(cpu.ime, Disabled);
        cpu.step();
        // ime should still be false
        assert_eq!(cpu.ime, PendingEnable);
        cpu.step();
        // IME should be set now:
        assert_eq!(cpu.ime, Enabled);
        cpu.step();
        assert_eq!(cpu.ime, Enabled);
    }

    #[test]
    /// EI sets the IME register, but the effect is only visible after the instruction following EI is executed. If the following instruction is DI, IME will remain unset
    fn ei_di_unset_ime() {
        use ImeState::*;
        // Program is
        // EI
        // DI
        // NOP
        let mut cpu = Cpu::new(&[0xFB, 0xF3, 0x00], None, false);
        cpu.mmu.in_boot_rom = false;
        assert_eq!(cpu.ime, Disabled);
        cpu.step();
        // ime should be pending
        assert_eq!(cpu.ime, PendingEnable);
        cpu.step();
        assert_eq!(cpu.ime, Disabled);
        cpu.step();
        assert_eq!(cpu.ime, Disabled);
    }

    proptest! {
        #[test]
        fn sub_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::new(&FAKE_ROM, None, false);
            for flag in [Z, N, H, C] {
                cpu.regs.set_flag(flag, init_flags);
            }
            cpu.regs.a = a;
            cpu.sub_a_r8(R8::A);
            prop_assert_eq!(cpu.regs.a, 0);
            prop_assert_eq!(cpu.regs.flag(Z), true);
            prop_assert_eq!(cpu.regs.flag(N), true);
            prop_assert_eq!(cpu.regs.flag(H), false);
            prop_assert_eq!(cpu.regs.flag(C), false);
        }

        #[test]
        fn xor_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::new(&FAKE_ROM,None, false);
            for flag in [Z, N, H, C] {
                cpu.regs.set_flag(flag, init_flags);
            }
            cpu.regs.a = a;
            cpu.xor_a_r8(R8::A);
            prop_assert_eq!(cpu.regs.a, 0);
            prop_assert_eq!(cpu.regs.flag(Z), true);
            prop_assert_eq!(cpu.regs.flag(N), false);
            prop_assert_eq!(cpu.regs.flag(H), false);
            prop_assert_eq!(cpu.regs.flag(C), false);
        }

        #[test]
        fn or_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::new(&FAKE_ROM, None, false);
            for flag in [Z, N, H, C] {
                cpu.regs.set_flag(flag, init_flags);
            }
            cpu.regs.a = a;
            cpu.or_a_r8(R8::A);
            prop_assert_eq!(cpu.regs.a, a);
            prop_assert_eq!(cpu.regs.flag(Z), a == 0);
            prop_assert_eq!(cpu.regs.flag(N), false);
            prop_assert_eq!(cpu.regs.flag(H), false);
            prop_assert_eq!(cpu.regs.flag(C), false);
        }

        #[test]
        fn and_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::new(&FAKE_ROM, None, false);
            for flag in [Z, N, H, C] {
                cpu.regs.set_flag(flag, init_flags);
            }
            cpu.regs.a = a;
            cpu.and_a_r8(R8::A);
            prop_assert_eq!(cpu.regs.a, a);
            prop_assert_eq!(cpu.regs.flag(Z), a==0);
            prop_assert_eq!(cpu.regs.flag(N), false);
            prop_assert_eq!(cpu.regs.flag(H), true);
            prop_assert_eq!(cpu.regs.flag(C), false);
        }

        #[test]
        fn cp_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::new(&FAKE_ROM,None, false);
            for flag in [Z, N, H, C] {
                cpu.regs.set_flag(flag, init_flags);
            }
            cpu.regs.a = a;
            cpu.cp_a_r8(R8::A);
            prop_assert_eq!(cpu.regs.a, a);
            prop_assert_eq!(cpu.regs.flag(Z), true);
            prop_assert_eq!(cpu.regs.flag(N), true);
            prop_assert_eq!(cpu.regs.flag(H), false);
            prop_assert_eq!(cpu.regs.flag(C), false);
        }
    }
}
