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
/// Each function simulates the execution of an instruction and returns the number of T-cycles it takes
impl Cpu {
    // TODO: its possible that I can inline the register A as the argument to most of the arithmetic functions...

    // --- 8-bit Arithmetic and Logic Instructions

    /// Add the 8-bit values and the carry flag, set flags, and return the result
    fn alu_adc(&mut self, x: u8, y: u8) -> u8 {
        self.alu_add(x, y, self.regs.get_flag(Flag::C))
    }

    /// Add the 8-bit values and the carry bit, set flags, and return the result
    fn alu_add(&mut self, x: u8, y: u8, carry: bool) -> u8 {
        use Flag::*;
        let carry = self.regs.get_flag(C) as u8;
        let result = x
            .wrapping_add(y)
            .wrapping_add(self.regs.get_flag(Flag::C) as u8);

        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs
            .set_flag(C, (x as u16 + y as u16 + carry as u16) > (u8::MAX as u16));
        self.regs
            .set_flag(H, (x & 0x0f) + (y & 0x0f) + carry > 0x0f);
        result
    }

    /// ADC A,r8
    pub fn adc_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_adc(self.regs.a, self.regs.read(r));
        4
    }

    /// ADC A,\[HL\]
    pub fn adc_a_ref_hl(&mut self) -> u8 {
        self.regs.a = self.alu_adc(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// ADC A,n8
    pub fn adc_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.regs.a = self.alu_adc(self.regs.a, imm);
        8
    }

    /// ADD A,r8
    pub fn add_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_add(self.regs.a, self.regs.read(r), false);
        4
    }

    /// ADD A,\[HL\]
    pub fn add_a_ref_hl(&mut self) -> u8 {
        self.regs.a = self.alu_add(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// ADD A,n8
    pub fn add_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.regs.a = self.alu_add(self.regs.a, imm, false);
        8
    }

    /// AND the 8-bit values, set flags, and return the result
    fn alu_and(&mut self, x: u8, y: u8) -> u8 {
        use Flag::*;
        let result = x & y;
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, true);
        self.regs.set_flag(C, false);
        result
    }

    /// AND A,r8
    pub fn and_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_and(self.regs.a, self.regs.read(r));
        4
    }

    /// AND A,\[HL\]
    pub fn and_a_ref_hl(&mut self) -> u8 {
        self.regs.a = self.alu_and(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// AND A,n8
    pub fn and_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.regs.a = self.alu_and(self.regs.a, imm);
        8
    }

    /// Subtract the carry flag and y from x, set flags accordingly, and return the result
    fn alu_sub(&mut self, x: u8, y: u8, carry: bool) -> u8 {
        use Flag::*;
        let result = x.wrapping_sub(y).wrapping_sub(carry as u8);
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, true);
        self.regs.set_flag(
            H,
            (x & 0x0f).wrapping_sub(y & 0xf).wrapping_sub(carry as u8) & 0x10 != 0,
        );
        self.regs.set_flag(C, y as u16 + carry as u16 > x as u16);
        result
    }

    /// CP A,r8
    ///
    /// Subtract the value in r8 from A and set flags accordingly, but don't store the result.
    /// This is useful for ComParing values.
    pub fn cp_a_r8(&mut self, r: R8) -> u8 {
        self.alu_sub(self.regs.a, self.regs.read(r), false);
        4
    }

    /// CP A,\[HL\]
    pub fn cp_a_ref_hl(&mut self) -> u8 {
        self.alu_sub(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// CP A,n8
    pub fn cp_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.alu_sub(self.regs.a, imm, false);
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
        let result = self.alu_dec(self.regs.read(r));
        self.regs.write(r, result);
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
        let result = self.alu_inc(self.regs.read(r));
        self.regs.write(r, result);
        4
    }

    /// INC \[HL\]
    pub fn inc_ref_hl(&mut self) -> u8 {
        let result = self.alu_inc(self.mmu.read_byte(self.regs.hl()));
        self.mmu.write_byte(self.regs.hl(), result);
        12
    }

    /// ORs the values, sets flags, and returns the result
    fn alu_or(&mut self, x: u8, y: u8) -> u8 {
        use Flag::*;
        let result = x | y;
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, false);
        result
    }

    /// OR A,r8
    pub fn or_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_or(self.regs.a, self.regs.read(r));
        self.regs.a = result;
        4
    }

    /// OR A,\[HL\]
    pub fn or_a_ref_hl(&mut self) -> u8 {
        let result = self.alu_or(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        self.regs.a = result;
        8
    }

    /// OR A,n8
    pub fn or_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        let result = self.alu_or(self.regs.a, imm);
        self.regs.a = result;
        8
    }

    /// SBC A,r8
    pub fn sbc_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_sub(self.regs.a, self.regs.read(r), self.regs.get_flag(Flag::C));
        self.regs.a = result;
        4
    }

    /// SBC A,\[HL\]
    pub fn sbc_a_ref_hl(&mut self) -> u8 {
        let result = self.alu_sub(
            self.regs.a,
            self.mmu.read_byte(self.regs.hl()),
            self.regs.get_flag(Flag::C),
        );
        self.regs.a = result;
        8
    }

    /// SBC A,n8
    pub fn sbc_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        let result = self.alu_sub(self.regs.a, imm, self.regs.get_flag(Flag::C));
        self.regs.a = result;
        8
    }

    /// SUB A,r8
    pub fn sub_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_sub(self.regs.a, self.regs.read(r), false);
        self.regs.a = result;
        4
    }

    /// SUB A,\[HL\]
    pub fn sub_a_ref_hl(&mut self) -> u8 {
        let result = self.alu_sub(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        self.regs.a = result;
        8
    }

    /// SUB A,n8
    pub fn sub_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        let result = self.alu_sub(self.regs.a, imm, false);
        self.regs.a = result;
        8
    }

    /// XORs the two values, sets flags, and return the result
    fn alu_xor(&mut self, x: u8, y: u8) -> u8 {
        use Flag::*;
        let result = x ^ y;
        self.regs.set_flag(Z, result == 0);
        self.regs.set_flag(N, false);
        self.regs.set_flag(H, false);
        self.regs.set_flag(C, false);
        result
    }

    /// XOR A,r8
    pub fn xor_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_xor(self.regs.a, self.regs.read(r));
        self.regs.a = result;
        4
    }

    /// XOR A,\[HL\]
    pub fn xor_a_ref_hl(&mut self) -> u8 {
        let result = self.alu_xor(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        self.regs.a = result;
        8
    }

    /// XOR A,n8
    pub fn xor_a_n8(&mut self) -> u8 {
        let imm = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        self.regs.a = self.alu_xor(self.regs.a, imm);
        8
    }

    // --- 16-bit Arithmetic Instructions ---

    /// ADD HL,r16
    pub fn add_hl_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// DEC r16
    pub fn dec_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// INC r16
    pub fn inc_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    // --- Bit Operations Instructions ---

    /// BIT u3,r8
    pub fn bit_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// BIT u3,\[HL\]
    pub fn bit_u3_ref_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// RES u3,r8
    pub fn res_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// RES u3,\[HL\]
    pub fn res_u3_ref_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// SET u3,r8
    pub fn set_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// SET u3,\[HL\]
    pub fn set_u3_ref_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// SWAP r8
    pub fn swap_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SWAP \[HL\]
    pub fn swap_ref_hl(&mut self) -> u8 {
        todo!()
    }

    // --- Bit Shift Instructions ---

    /// RL r8
    pub fn rl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RL \[HL\]
    pub fn rl_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// RLA
    pub fn rla(&mut self) -> u8 {
        todo!()
    }

    /// RLC r8
    pub fn rlc_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RLC \[HL\]
    pub fn rlc_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// RLCA
    pub fn rlca(&mut self) -> u8 {
        todo!()
    }

    /// RR r8
    pub fn rr_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RR \[HL\]
    pub fn rr_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// RRA
    pub fn rra(&mut self) -> u8 {
        todo!()
    }

    /// RRC r8
    pub fn rrc_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RRC \[HL\]
    pub fn rrc_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// RRCA
    pub fn rrca(&mut self) -> u8 {
        todo!()
    }

    /// SLA r8
    pub fn sla_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SLA \[HL\]
    pub fn sla_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// SRA r8
    pub fn sra_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SRA \[HL\]
    pub fn sra_ref_hl(&mut self) -> u8 {
        todo!()
    }

    /// SRL r8
    pub fn srl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SRL \[HL\]
    pub fn srl_ref_hl(&mut self) -> u8 {
        todo!()
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
