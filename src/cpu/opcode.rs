use super::{
    register_file::{Flag, R16, R8},
    Cpu,
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
    // TODO: its possible that I can inline the register A as the argument to most of the arithmetic functions...

    // --- 8-bit Arithmetic and Logic Instructions

    /// Add the value and carry flag to A, and set flags accordingly
    fn alu_adc(&mut self, x: u8) {
        self.alu_add(x, self.regs.flag(Flag::C));
    }

    /// Add the value and carry bit to A, and set flags accordingly
    fn alu_add(&mut self, x: u8, carry: bool) {
        use Flag::*;
        let carry = self.regs.flag(C) as u8;
        let a = self.regs.a;
        let result = a
            .wrapping_add(x)
            .wrapping_add(self.regs.flag(Flag::C) as u8);

        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs
            .set_flag(C, (a as u16 + x as u16 + carry as u16) > (u8::MAX as u16));
        self.regs
            .set_flag(H, (a & 0x0f) + (x & 0x0f) + carry > 0x0f);
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        self.regs.set_flag(
            H,
            (a & 0x0f).wrapping_sub(x & 0xf).wrapping_sub(carry as u8) & 0x10 != 0,
        );
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.alu_cp(imm);
        8
    }

    /// Decrements the value by 1, sets flags, and returns the result
    fn alu_dec(&mut self, x: u8) -> u8 {
        use Flag::*;
        let result = x.wrapping_sub(1);
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, true);
        self.regs.set_flag(H, x & 0x0f == 0);
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
        self.regs.set_flag(H, val & 0x0f == 0x0f);
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
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
        self.regs.set_flag(C, hl > 0xffff - val);
        // set half-carry if overflow from bit 11
        let mask = 0x07ff;
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
        let lower = val & 0xf;
        let upper = val & 0xf0;
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
    /// ```
    ///   ┏━ Flags ━┓ ┏━━━━━━━ u8 ━━━━━━┓
    /// ┌─╂─   C   ←╂─╂─ b7 ← ... ← b0 ←╂─┐
    /// │ ┗━━━━━━━━━┛ ┗━━━━━━━━━━━━━━━━━┛ │
    /// └─────────────────────────────────┘
    /// ```
    fn alu_rl(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val.rotate_left(1) | (self.regs.flag(C) as u8);
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
    /// ```
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
    /// ```
    ///   ┏━━━━━━━ u8 ━━━━━━┓ ┏━ Flags ━┓
    /// ┌─╂→ b7 → ... → b0 ─╂─╂→   C   ─╂─┐
    /// │ ┗━━━━━━━━━━━━━━━━━┛ ┗━━━━━━━━━┛ │
    /// └─────────────────────────────────┘
    /// ```
    pub fn alu_rr(&mut self, val: u8) -> u8 {
        use Flag::*;
        let res = val.rotate_right(1) | ((self.regs.flag(C) as u8) << 7);
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
    /// ```
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
    ///```
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
    ///```
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
    ///```
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
        todo!()
    }

    /// LD r8,n8
    pub fn ld_r8_n8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// LD r16,n16
    pub fn ld_r16_n16(&mut self, r: R16) -> u8 {
        let word = self.mmu.read_word(self.regs.pc);
        self.regs.pc += 2;
        self.regs.set_r16(r, word);
        self.regs.pc += 2;
        12
    }

    /// LD \[HL\],r8
    pub fn ld_ref_hl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// LD \[HL\],n8
    pub fn ld_ref_hl_n8(&mut self) -> u8 {
        todo!()
    }

    /// LD r8,\[HL\]
    pub fn ld_r8_ref_hl(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// LD \[r16\],A
    pub fn ld_ref_r16_a(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// LD \[n16\],A
    pub fn ld_ref_n16_a(&mut self) -> u8 {
        todo!()
    }

    /// LDH \[n16\],A
    pub fn ldh_ref_n16_a(&mut self) -> u8 {
        todo!()
    }

    /// LDH \[C\],A
    pub fn ldh_ref_c_a(&mut self) -> u8 {
        todo!()
    }

    /// LD A,\[r16\]
    pub fn ld_a_ref_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// LD A,\[n16\]
    pub fn ld_a_ref_n16(&mut self) -> u8 {
        todo!()
    }

    /// LDH A,\[n16\]
    pub fn ldh_a_ref_n16(&mut self, addr: u8) -> u8 {
        todo!()
    }

    /// LDH A,\[C\]
    pub fn ldh_a_ref_c(&mut self) -> u8 {
        todo!()
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
        todo!()
    }

    /// LD A,\[HLD\]
    pub fn ld_a_ref_hld(&mut self) -> u8 {
        todo!()
    }

    // --- Jumps and Subroutines ---

    /// CALL n16
    pub fn call_n16(&mut self) -> u8 {
        todo!()
    }

    /// CALL cc,n16
    pub fn call_cc_n16(&mut self, cc: CC) -> u8 {
        todo!()
    }

    /// JP HL
    pub fn jp_hl(&mut self) -> u8 {
        todo!()
    }

    /// JP n16
    pub fn jp_n16(&mut self) -> u8 {
        todo!()
    }

    /// JP cc,n16
    pub fn jp_cc_n16(&mut self, cc: CC) -> u8 {
        todo!()
    }

    /// JR n16
    pub fn jr_n16(&mut self) -> u8 {
        todo!()
    }

    /// JR cc,n16
    pub fn jr_cc_n16(&mut self, cc: CC) -> u8 {
        todo!()
    }

    /// RET cc
    pub fn ret_cc(&mut self, cc: CC) -> u8 {
        todo!()
    }

    /// RET
    pub fn ret(&mut self) -> u8 {
        todo!()
    }

    /// RETI
    pub fn reti(&mut self) -> u8 {
        todo!()
    }

    /// RST vec
    pub fn rst_vec(&mut self, vec: RstVec) -> u8 {
        todo!()
    }

    // --- Stack Operations Instructions ---

    /// ADD HL,SP
    fn add_hl_sp(&mut self) -> u8 {
        todo!()
    }

    /// ADD SP,e8
    pub fn add_sp_e8(&mut self) -> u8 {
        todo!()
    }

    /// DEC SP
    fn dec_sp(&mut self) -> u8 {
        todo!()
    }

    /// INC SP
    fn inc_sp(&mut self) -> u8 {
        todo!()
    }

    /// LD SP,n16
    fn ld_sp_n16(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// LD [n16],SP
    fn ld_n16_sp(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// LD HL,SP+e8
    pub fn ld_hl_sp_e8(&mut self) -> u8 {
        todo!()
    }

    /// LD SP,HL
    pub fn ld_sp_hl(&mut self) -> u8 {
        todo!()
    }

    /// POP AF
    fn pop_af(&mut self) -> u8 {
        todo!()
    }

    /// POP r16
    pub fn pop_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// PUSH AF
    fn push_af(&mut self) -> u8 {
        todo!()
    }

    /// PUSH r16
    pub fn push_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    pub fn daa(&mut self) -> u8 {
        todo!()
    }

    pub fn scf(&mut self) -> u8 {
        todo!()
    }

    pub fn ccf(&mut self) -> u8 {
        todo!()
    }

    pub fn cpl(&mut self) -> u8 {
        todo!()
    }

    pub fn halt(&mut self) -> u8 {
        todo!()
    }

    pub fn di(&mut self) -> u8 {
        todo!()
    }

    pub fn ei(&mut self) -> u8 {
        todo!()
    }

    pub fn nop(&self) -> u8 {
        todo!()
    }

    pub fn stop(&self) -> u8 {
        todo!()
    }
}

#[cfg(test)]
mod tests {
    use proptest::{prop_assert_eq, proptest};

    use crate::cpu::{
        register_file::{Flag, R8},
        Cpu,
    };

    proptest! {
        #[test]
        fn sub_a_a(a: u8, init_flags: bool) {
            use Flag::*;
            let mut cpu = Cpu::create();
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
            let mut cpu = Cpu::create();
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
            let mut cpu = Cpu::create();
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
            let mut cpu = Cpu::create();
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
            let mut cpu = Cpu::create();
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
