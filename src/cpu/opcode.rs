use super::{
    register_file::{Flag, R16, R8},
    CPU,
};

enum ConditionCode {
    /// Execute if Z is set
    Z,
    /// Execute if Z is not set
    NZ,
    /// Execute if C is set
    C,
    /// Execute if C is not set
    NC,
}

/// Implementation of all cpu instruction.
/// Each function simulates the execution of an instruction and returns the number of T-cycles it takes
impl CPU {
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
        self.regs.set_flag(H, x & 0x0f + y & 0x0f + carry > 0x0f);
        result
    }

    /// ADC A,r8
    fn adc_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_adc(self.regs.a, self.regs.read(r));
        4
    }

    /// ADC A,[HL]
    fn adc_a_hl(&mut self) -> u8 {
        self.regs.a = self.alu_adc(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// ADC A,n8
    fn adc_a_n8(&mut self, imm: u8) -> u8 {
        self.regs.a = self.alu_adc(self.regs.a, imm);
        8
    }

    /// ADD A,r8
    fn add_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_add(self.regs.a, self.regs.read(r), false);
        4
    }

    /// ADD A,[HL]
    fn add_a_hl(&mut self) -> u8 {
        self.regs.a = self.alu_add(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// ADD A,n8
    fn add_a_n8(&mut self, imm: u8) -> u8 {
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
    fn and_a_r8(&mut self, r: R8) -> u8 {
        self.regs.a = self.alu_and(self.regs.a, self.regs.read(r));
        4
    }

    /// AND A,[HL]
    fn and_a_hl(&mut self) -> u8 {
        self.regs.a = self.alu_and(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        8
    }

    /// AND A,n8
    fn and_a_n8(&mut self, imm: u8) -> u8 {
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

    /// Subtract the value in r8 from A and set flags accordingly, but don't store the result.
    /// This is useful for ComParing values.
    /// CP A,r8
    fn cp_a_r8(&mut self, r: R8) -> u8 {
        self.alu_sub(self.regs.a, self.regs.read(r), false);
        4
    }

    /// CP A,[HL]
    fn cp_a_hl(&mut self) -> u8 {
        self.alu_sub(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        8
    }

    /// CP A,n8
    fn cp_a_n8(&mut self, imm: u8) -> u8 {
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
    fn dec_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_dec(self.regs.read(r));
        self.regs.write(r, result);
        4
    }

    /// DEC [HL]
    fn dec_hl(&mut self) -> u8 {
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
    fn inc_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_inc(self.regs.read(r));
        self.regs.write(r, result);
        4
    }

    /// INC [HL]
    fn inc_hl(&mut self) -> u8 {
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
    fn or_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_or(self.regs.a, self.regs.read(r));
        self.regs.a = result;
        4
    }

    /// OR A,[HL]
    fn or_a_hl(&mut self) -> u8 {
        let result = self.alu_or(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        self.regs.a = result;
        8
    }

    /// OR A,n8
    fn or_a_n8(&mut self, imm: u8) -> u8 {
        let result = self.alu_or(self.regs.a, imm);
        self.regs.a = result;
        8
    }

    /// SBC A,r8
    fn sbc_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_sub(self.regs.a, self.regs.read(r), self.regs.get_flag(Flag::C));
        self.regs.a = result;
        4
    }

    /// SBC A,[HL]
    fn sbc_a_hl(&mut self) -> u8 {
        let result = self.alu_sub(
            self.regs.a,
            self.mmu.read_byte(self.regs.hl()),
            self.regs.get_flag(Flag::C),
        );
        self.regs.a = result;
        8
    }

    /// SBC A,n8
    fn sbc_a_n8(&mut self, imm: u8) -> u8 {
        let result = self.alu_sub(self.regs.a, imm, self.regs.get_flag(Flag::C));
        self.regs.a = result;
        8
    }

    /// SUB A,r8
    fn sub_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_sub(self.regs.a, self.regs.read(r), false);
        self.regs.a = result;
        4
    }

    /// SUB A,[HL]
    fn sub_a_hl(&mut self) -> u8 {
        let result = self.alu_sub(self.regs.a, self.mmu.read_byte(self.regs.hl()), false);
        self.regs.a = result;
        8
    }

    /// SUB A,n8
    fn sub_a_n8(&mut self, imm: u8) -> u8 {
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
    fn xor_a_r8(&mut self, r: R8) -> u8 {
        let result = self.alu_xor(self.regs.a, self.regs.read(r));
        self.regs.a = result;
        4
    }

    /// XOR A,[HL]
    fn xor_a_hl(&mut self) -> u8 {
        let result = self.alu_xor(self.regs.a, self.mmu.read_byte(self.regs.hl()));
        self.regs.a = result;
        8
    }

    /// XOR A,n8
    fn xor_a_n8(&mut self, imm: u8) -> u8 {
        self.regs.a = self.alu_xor(self.regs.a, imm);
        8
    }

    // --- 16-bit Arithmetic Instructions ---

    /// ADD HL,r16
    fn add_hl_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// DEC r16
    fn dec_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// INC r16
    fn inc_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    // --- Bit Operations Instructions ---

    /// BIT u3,r8
    fn bit_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// BIT u3,[HL]
    fn bit_u3_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// RES u3,r8
    fn res_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// RES u3,[HL]
    fn res_u3_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// SET u3,r8
    fn set_u3_r8(&mut self, u3: u8, reg: R8) -> u8 {
        todo!()
    }

    /// SET u3,[HL]
    fn set_u3_hl(&mut self, u3: u8) -> u8 {
        todo!()
    }

    /// SWAP r8
    fn swap_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SWAP [HL]
    fn swap_hl(&mut self) -> u8 {
        todo!()
    }

    // --- Bit Shift Instructions ---

    /// RL r8
    fn rl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RL [HL]
    fn rl_hl(&mut self) -> u8 {
        todo!()
    }

    /// RLA
    fn rla(&mut self) -> u8 {
        todo!()
    }

    /// RLC r8
    fn rlc_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RLC [HL]
    fn rlc_hl(&mut self) -> u8 {
        todo!()
    }

    /// RLCA
    fn rlca(&mut self) -> u8 {
        todo!()
    }

    /// RR r8
    fn rr_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RR [HL]
    fn rr_hl(&mut self) -> u8 {
        todo!()
    }

    /// RRA
    fn rra(&mut self) -> u8 {
        todo!()
    }

    /// RRC r8
    fn rrc_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// RRC [HL]
    fn rrc_hl(&mut self) -> u8 {
        todo!()
    }

    /// RRCA
    fn rrca(&mut self) -> u8 {
        todo!()
    }

    /// SLA r8
    fn sla_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SLA [HL]
    fn sla_hl(&mut self) -> u8 {
        todo!()
    }

    /// SRA r8
    fn sra_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SRA [HL]
    fn sra_hl(&mut self) -> u8 {
        todo!()
    }

    /// SRL r8
    fn srl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// SRL [HL]
    fn srl_hl(&mut self) -> u8 {
        todo!()
    }

    // --- Load Instructions ---

    /// LD r8,r8
    fn ld_r8_r8(&mut self, dest: R8, src: R8) -> u8 {
        todo!()
    }

    /// LD r8,n8
    fn ld_r8_n8(&mut self, reg: R8, imm: u8) -> u8 {
        todo!()
    }

    /// LD r16,n16
    fn ld_r16_n16(&mut self, reg: R16, imm: u16) -> u8 {
        todo!()
    }

    /// LD [HL],r8
    fn ld_hl_r8(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// LD [HL],n8
    fn ld_hl_n8(&mut self, imm: u8) -> u8 {
        todo!()
    }

    /// LD r8,[HL]
    fn ld_r8_hl(&mut self, reg: R8) -> u8 {
        todo!()
    }

    /// LD [r16],A
    fn ld_r16_a(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// LD [n16],A
    fn ld_n16_a(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// LDH [n16],A
    fn ldh_n16_a(&mut self, addr: u8) -> u8 {
        todo!()
    }

    /// LDH [C],A
    fn ldh_c_a(&mut self) -> u8 {
        todo!()
    }

    /// LD A,[r16]
    fn ld_a_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// LD A,[n16]
    fn ld_a_n16(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// LDH A,[n16]
    fn ldh_a_n16(&mut self, addr: u8) -> u8 {
        todo!()
    }

    /// LDH A,[C]
    fn ldh_a_c(&mut self) -> u8 {
        todo!()
    }

    /// LD [HLI],A
    fn ld_hli_a(&mut self) -> u8 {
        todo!()
    }

    /// LD [HLD],A
    fn ld_hld_a(&mut self) -> u8 {
        todo!()
    }

    /// LD A,[HLI]
    fn ld_a_hli(&mut self) -> u8 {
        todo!()
    }

    /// LD A,[HLD]
    fn ld_a_hld(&mut self) -> u8 {
        todo!()
    }

    // --- Jumps and Subroutines ---

    /// CALL n16
    fn call_n16(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// CALL cc,n16
    fn call_cc_n16(&mut self, cc: ConditionCode, addr: u16) -> u8 {
        todo!()
    }

    /// JP HL
    fn jp_hl(&mut self) -> u8 {
        todo!()
    }

    /// JP n16
    fn jp_n16(&mut self, addr: u16) -> u8 {
        todo!()
    }

    /// JP cc,n16
    fn jp_cc_n16(&mut self, cc: ConditionCode, addr: u16) -> u8 {
        todo!()
    }

    /// JR n16
    fn jr_n16(&mut self, offset: i8) -> u8 {
        todo!()
    }

    /// JR cc,n16
    fn jr_cc_n16(&mut self, cc: ConditionCode, offset: i8) -> u8 {
        todo!()
    }

    /// RET cc
    fn ret_cc(&mut self, cc: ConditionCode) -> u8 {
        todo!()
    }

    /// RET
    fn ret(&mut self) -> u8 {
        todo!()
    }

    /// RETI
    fn reti(&mut self) -> u8 {
        todo!()
    }

    /// RST vec
    fn rst_vec(&mut self, vec: u8) -> u8 {
        todo!()
    }

    // --- Stack Operations Instructions ---

    /// ADD HL,SP
    fn add_hl_sp(&mut self) -> u8 {
        todo!()
    }

    /// ADD SP,e8
    fn add_sp_e8(&mut self, offset: i8) -> u8 {
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
    fn ld_hl_sp_e8(&mut self, offset: i8) -> u8 {
        todo!()
    }

    /// LD SP,HL
    fn ld_sp_hl(&mut self) -> u8 {
        todo!()
    }

    /// POP AF
    fn pop_af(&mut self) -> u8 {
        todo!()
    }

    /// POP r16
    fn pop_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }

    /// PUSH AF
    fn push_af(&mut self) -> u8 {
        todo!()
    }

    /// PUSH r16
    fn push_r16(&mut self, reg: R16) -> u8 {
        todo!()
    }
}
