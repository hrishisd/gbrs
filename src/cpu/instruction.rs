use crate::cpu::register_file::R16;
use crate::cpu::register_file::R8;

/// A decoded instruction.
///
/// ref: https://rgbds.gbdev.io/docs/v0.8.0
#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Instruction {
    // --- 8-bit arithmetic and logic instructions ---
    ADC_A(Operand),
    ADD_A(Operand),
    AND_A(Operand),
    CP_A(Operand),
    DEC(HlOrReg8),
    INC(HlOrReg8),
    OR_A(Operand),
    SBC_A(Operand),
    SUB_A(Operand),
    XOR_A(Operand),

    // --- 16-bit arithmetic instructions ---
    ADD_HL(R16),
    DEC16(R16),
    INC16(R16),

    // --- bit ops instructions ---
    BIT(U3, HlOrReg8),
    RES(U3, HlOrReg8),
    SET(U3, HlOrReg8),
    SWAP(HlOrReg8),

    // --- bit shift instructions ---
    // TODO: take care in implementation of RLA, RLCA, RRA, and RRCA since flag behavior can vary across implementations
    // TODO: should RLA, RLCA, etc be separate instructions?
    RL(HlOrReg8),
    RLA,
    RLC(HlOrReg8),
    RLCA,
    RR(HlOrReg8),
    RRA,
    RRC(HlOrReg8),
    RRCA,
    SLA(HlOrReg8),
    SRA(HlOrReg8),
    SRL(HlOrReg8),

    // --- load instructions ---
    /// LD r8,*
    LD_R8(R8, Operand),
    /// LD [HL],*
    LD_HL(ImmOrR8),
    /// LD r16,n16
    LD_R16_N16(R16, u16),
    /// LD [r16],A
    LD_ADDR_R16(R16),
    /// LD [n16],A
    LD_ADDR_N16(u16),
    /// LDH [n16],A
    LDH_N16_A,
    /// LDH [C],A
    LDH_C_A,
    /// LD A,[r16]
    /// LD A,[n16]
    LD_A(COrN16),
    LDH_A(COrN16),
    LD_HL_A(HLIncOrDec),
    LD_A_HL(HLIncOrDec),

    // --- jumps and subroutines ---
    CALL(u16),
    CALL_CC(ConditionCode, u16),
    JP_HL,
    JP_N16(u16),
    // TODO: when decoding the JR instructions, make sure to calculate the address N16, properly given a relative jump of type i8 from the current address
    JP_CC_N16(ConditionCode, u16),
    JR(u16),
    JR_CC(ConditionCode, u16),
    RET_CC(ConditionCode),
    RET,
    RETI,
    RST(RstVec),

    // --- stack operation instructions
    ADD_HL_SP,
    ADD_SP(i8),
    DEC_SP,
    INC_SP,
    LD_SP_N16(u16),
    /// LD [n16],SP
    LD_ADDR_N16_SP(u16),
    /// LD HL,SP+e8
    LD_HL_SP_E8(i8),
    LD_SP_HL,
    POP_AF,
    POP_R16(R16),
    PUSH_AF,
    PUSH_R16(R16),

    // --- miscellaneous instructions
    CCF,
    CPL,
    DAA,
    DI,
    EI,
    HALT,
    NOP,
    SCF,
    STOP,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HLIncOrDec {
    /// Post-increment HL
    HLI,
    /// Post-decrement HL
    HLD,
}

#[allow(non_camel_case_types)]
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum COrN16 {
    /// The value at the byte address 0xFF00 + C.
    FF_C,
    /// A 16 bit immediate value
    N16,
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ImmOrR8 {
    /// An 8-bit register.
    Reg(R8),
    /// An 8-bit immediate value.
    N8(u8),
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum AOrHLOrR8 {
    /// The accumualtor register
    A,
    /// The byte pointed to be HL.
    /// Also encoded as [HL].
    HL,
    /// An 8-bit register.
    Reg(R8),
}

/// For instructions that operate on either [HL] or an 8-bit register.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum HlOrReg8 {
    /// The byte pointed to be HL.
    /// Also encoded as [HL].
    HL,
    /// An 8-bit register.
    Reg(R8),
}

/// Possible operands for 8-bit arithmetic and logic instructions involving the accumulator register, A.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Operand {
    /// An 8-bit register.
    Reg(R8),
    /// An 8-bit immediate value.
    Imm(u8),
    /// The byte pointed to be HL.
    /// Also encoded as [HL].
    HL,
}

/// Represents a 3-bit integer constant (0 to 7).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub struct U3(u8);

impl U3 {
    pub fn new(value: u8) -> Self {
        debug_assert!(value <= 7, "U3 can only represent values 0-7.");
        Self(value)
    }
}

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

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum ConditionCode {
    /// Execute if Z is set
    Z,
    /// Execute if Z is not set
    NZ,
    /// Execute if C is set
    C,
    /// Execute if C is not set
    NC,
}

impl Instruction {
    fn cycles(self) -> u8 {
        match self {
            Instruction::ADC_A(operand) => todo!(),
            Instruction::ADD_A(operand) => todo!(),
            Instruction::AND_A(operand) => todo!(),
            Instruction::CP_A(operand) => todo!(),
            Instruction::DEC(hl_or_reg8) => todo!(),
            Instruction::INC(hl_or_reg8) => todo!(),
            Instruction::OR_A(operand) => todo!(),
            Instruction::SBC_A(operand) => todo!(),
            Instruction::SUB_A(operand) => todo!(),
            Instruction::XOR_A(operand) => todo!(),
            Instruction::ADD_HL(r16) => todo!(),
            Instruction::DEC16(r16) => todo!(),
            Instruction::INC16(r16) => todo!(),
            Instruction::BIT(u3, hl_or_reg8) => todo!(),
            Instruction::RES(u3, hl_or_reg8) => todo!(),
            Instruction::SET(u3, hl_or_reg8) => todo!(),
            Instruction::SWAP(hl_or_reg8) => todo!(),
            Instruction::RL(hl_or_reg8) => todo!(),
            Instruction::RLA => todo!(),
            Instruction::RLC(hl_or_reg8) => todo!(),
            Instruction::RLCA => todo!(),
            Instruction::RR(hl_or_reg8) => todo!(),
            Instruction::RRA => todo!(),
            Instruction::RRC(hl_or_reg8) => todo!(),
            Instruction::RRCA => todo!(),
            Instruction::SLA(hl_or_reg8) => todo!(),
            Instruction::SRA(hl_or_reg8) => todo!(),
            Instruction::SRL(hl_or_reg8) => todo!(),
            Instruction::LD_R8(r8, operand) => todo!(),
            Instruction::LD_HL(imm_or_r8) => todo!(),
            Instruction::LD_R16_N16(r16, _) => todo!(),
            Instruction::LD_ADDR_R16(r16) => todo!(),
            Instruction::LD_ADDR_N16(_) => todo!(),
            Instruction::LDH_N16_A => todo!(),
            Instruction::LDH_C_A => todo!(),
            Instruction::LD_A(cor_n16) => todo!(),
            Instruction::LDH_A(cor_n16) => todo!(),
            Instruction::LD_HL_A(hlinc_or_dec) => todo!(),
            Instruction::LD_A_HL(hlinc_or_dec) => todo!(),
            Instruction::CALL(_) => todo!(),
            Instruction::CALL_CC(condition_code, _) => todo!(),
            Instruction::JP_HL => todo!(),
            Instruction::JP_N16(_) => todo!(),
            Instruction::JP_CC_N16(condition_code, _) => todo!(),
            Instruction::JR(_) => todo!(),
            Instruction::JR_CC(condition_code, _) => todo!(),
            Instruction::RET_CC(condition_code) => todo!(),
            Instruction::RET => todo!(),
            Instruction::RETI => todo!(),
            Instruction::RST(rst_vec) => todo!(),
            Instruction::ADD_HL_SP => todo!(),
            Instruction::ADD_SP(_) => todo!(),
            Instruction::DEC_SP => todo!(),
            Instruction::INC_SP => todo!(),
            Instruction::LD_SP_N16(_) => todo!(),
            Instruction::LD_ADDR_N16_SP(_) => todo!(),
            Instruction::LD_HL_SP_E8(_) => todo!(),
            Instruction::LD_SP_HL => todo!(),
            Instruction::POP_AF => todo!(),
            Instruction::POP_R16(r16) => todo!(),
            Instruction::PUSH_AF => todo!(),
            Instruction::PUSH_R16(r16) => todo!(),
            Instruction::CCF => todo!(),
            Instruction::CPL => todo!(),
            Instruction::DAA => todo!(),
            Instruction::DI => todo!(),
            Instruction::EI => todo!(),
            Instruction::HALT => todo!(),
            Instruction::NOP => todo!(),
            Instruction::SCF => todo!(),
            Instruction::STOP => todo!(),
        }
    }
}
