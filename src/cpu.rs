use std::{
    fs::File,
    io::{BufWriter, Write},
};

// #![allow(unused)]
use opcode::{RstVec, CC};
use register_file::{Registers, R16, R8};

use crate::mmu::{InterruptKind, Mmu};

mod opcode;
mod register_file;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImeState {
    Enabled,
    Disabled,
    PendingEnable,
}

pub struct Cpu {
    // TODO: remove pub
    pub regs: Registers,
    pub mmu: Mmu,
    /// AKA, the `IME` flag.
    ///
    /// `IME` is the main switch to enable/disable all interrupts. `IE` is more granular, and enables/disables interrupts individually depending on which bits are set.
    ime: ImeState,
    dbg_log_file: Option<BufWriter<File>>,
    is_halted: bool,
}

impl Cpu {
    pub fn create(rom: &[u8]) -> Self {
        Cpu {
            regs: Registers::create(),
            mmu: Mmu::create(rom),
            ime: ImeState::Disabled,
            dbg_log_file: None,
            is_halted: false,
        }
    }

    /// Initializes the CPU's state to the state it should have immediately after executing the boot ROM
    pub fn _debug_mode(test_rom: &[u8], dbg_log_file: BufWriter<File>) -> Self {
        let mut cpu = Cpu {
            regs: Registers {
                a: 0x01,
                f: 0xB0,
                b: 0x00,
                c: 0x13,
                d: 0x00,
                e: 0xD8,
                h: 0x01,
                l: 0x4D,
                sp: 0xFFFE,
                pc: 0x0100,
            },
            mmu: Mmu::_debug_mode(test_rom),
            ime: ImeState::Disabled,
            dbg_log_file: Some(dbg_log_file),
            is_halted: false,
        };
        if let Some(file) = &mut cpu.dbg_log_file {
            writeln!(
                file,
                    "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
                    cpu.regs.a, cpu.regs.f, cpu.regs.b, cpu.regs.c, cpu.regs.d, cpu.regs.e, cpu.regs.h, cpu.regs.l, cpu.regs.sp, cpu.regs.pc, cpu.mmu.read_byte(cpu.regs.pc), cpu.mmu.read_byte(cpu.regs.pc+1), cpu.mmu.read_byte(cpu.regs.pc+2), cpu.mmu.read_byte(cpu.regs.pc+3)
            )
                .unwrap();
        }
        cpu
    }

    /// Fetch, decode, and execute a single instruction.
    ///
    /// Returns the number of master clock cycles (at 4 MiHz) that the instruction takes.
    /// E.g. executing the `NOP` instruction will return 4
    pub fn step(&mut self) -> u8 {
        // TODO: if interrupt handled, update t_cycles for step
        // TODO: check interrupt handling implementation against:
        //     http://www.codeslinger.co.uk/pages/projects/gameboy/interrupts.html
        // handle interrupts
        // println!(
        //     "IME: {:?} HALTED: {:?}, IE: {:?}, IF: {:?}\nA:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
        //     self.ime, self.is_halted, self.mmu.interrupts_enabled, self.mmu.interrupts_requested, self.regs.a, self.regs.f, self.regs.b, self.regs.c, self.regs.d, self.regs.e, self.regs.h, self.regs.l, self.regs.sp, self.regs.pc, self.mmu.read_byte(self.regs.pc), self.mmu.read_byte(self.regs.pc+1), self.mmu.read_byte(self.regs.pc+2), self.mmu.read_byte(self.regs.pc+3));
        let mut handled_interrupt = false;
        if self.ime == ImeState::Enabled {
            use InterruptKind::*;
            for interrupt_kind in [Vblank, LcdStat, Serial, Timer, Joypad] {
                if self.mmu.interrupts_requested.contains(interrupt_kind)
                    && self.mmu.interrupts_enabled.contains(interrupt_kind)
                {
                    self.ime = ImeState::Disabled;
                    self.is_halted = false;
                    self.mmu.interrupts_requested.remove(interrupt_kind);
                    self.push_u16(self.regs.pc);
                    self.regs.pc = match interrupt_kind {
                        Joypad => 0x60,
                        Serial => 0x58,
                        Timer => 0x50,
                        LcdStat => 0x48,
                        Vblank => 0x40,
                    };
                    self.mmu.step(20);
                    handled_interrupt = true;
                    break;
                }
            }
        } else {
            let pending_interrupts = self.mmu.interrupts_requested & self.mmu.interrupts_enabled;
            if !pending_interrupts.is_empty() && self.is_halted {
                self.is_halted = false;
            }
        }

        // update ime state
        if self.ime == ImeState::PendingEnable {
            self.ime = ImeState::Enabled;
        }

        if self.is_halted {
            self.mmu.step(4);
            4
        } else {
            // execute opcode
            let opcode = self.mmu.read_byte(self.regs.pc);
            self.regs.pc += 1;
            let t_cycles = self.execute(opcode);
            assert!(t_cycles % 4 == 0 && t_cycles <= 24, "Unexpected number of t-cycles during execution of opcode {opcode:x} execution: {t_cycles}");
            if let Some(file) = &mut self.dbg_log_file {
                writeln!(
                    file,
                        "A:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
                        self.regs.a, self.regs.f, self.regs.b, self.regs.c, self.regs.d, self.regs.e, self.regs.h, self.regs.l, self.regs.sp, self.regs.pc, self.mmu.read_byte(self.regs.pc), self.mmu.read_byte(self.regs.pc+1), self.mmu.read_byte(self.regs.pc+2), self.mmu.read_byte(self.regs.pc+3)
                )
                .unwrap();
            }

            self.mmu.step(t_cycles);

            t_cycles + if handled_interrupt { 20 } else { 0 }
        }
    }

    /// Execute a single instruction and return the number of system clock cycles (T-cycles) the instruction takes.
    ///
    /// Precondition: PC points to the next byte after the opcode of the instruction being executed.
    ///
    /// While evaluating the opcode, `execute` will advance PC if the instruction consists of more bytes than just the opcode.
    /// ref: https://gbdev.io/gb-opcodes//optables/
    fn execute(&mut self, opcode: u8) -> u8 {
        match opcode {
            // --- Misc / control instructions ---
            0x00 => self.nop(),
            0x10 => self.stop(),
            0x27 => self.daa(),
            0x37 => self.scf(),
            0x2F => self.cpl(),
            0x3F => self.ccf(),
            0x76 => self.halt(),
            0xF3 => self.di(),
            0xFB => self.ei(),
            0xCB => {
                // TODO remember to account for prefix instruction when counting cycles
                let opcode = self.mmu.read_byte(self.regs.pc);
                self.regs.pc += 1;
                match opcode {
                    // rlc
                    0x00 => self.rlc_r8(R8::B),
                    0x01 => self.rlc_r8(R8::C),
                    0x02 => self.rlc_r8(R8::D),
                    0x03 => self.rlc_r8(R8::E),
                    0x04 => self.rlc_r8(R8::H),
                    0x05 => self.rlc_r8(R8::L),
                    0x06 => self.rlc_ref_hl(),
                    0x07 => self.rlc_r8(R8::A),
                    // rrc
                    0x08 => self.rrc_r8(R8::B),
                    0x09 => self.rrc_r8(R8::C),
                    0x0A => self.rrc_r8(R8::D),
                    0x0B => self.rrc_r8(R8::E),
                    0x0C => self.rrc_r8(R8::H),
                    0x0D => self.rrc_r8(R8::L),
                    0x0E => self.rrc_ref_hl(),
                    0x0F => self.rrc_r8(R8::A),
                    // rl
                    0x10 => self.rl_r8(R8::B),
                    0x11 => self.rl_r8(R8::C),
                    0x12 => self.rl_r8(R8::D),
                    0x13 => self.rl_r8(R8::E),
                    0x14 => self.rl_r8(R8::H),
                    0x15 => self.rl_r8(R8::L),
                    0x16 => self.rl_ref_hl(),
                    0x17 => self.rl_r8(R8::A),
                    // rr
                    0x18 => self.rr_r8(R8::B),
                    0x19 => self.rr_r8(R8::C),
                    0x1A => self.rr_r8(R8::D),
                    0x1B => self.rr_r8(R8::E),
                    0x1C => self.rr_r8(R8::H),
                    0x1D => self.rr_r8(R8::L),
                    0x1E => self.rr_ref_hl(),
                    0x1F => self.rr_r8(R8::A),
                    // sla
                    0x20 => self.sla_r8(R8::B),
                    0x21 => self.sla_r8(R8::C),
                    0x22 => self.sla_r8(R8::D),
                    0x23 => self.sla_r8(R8::E),
                    0x24 => self.sla_r8(R8::H),
                    0x25 => self.sla_r8(R8::L),
                    0x26 => self.sla_ref_hl(),
                    0x27 => self.sla_r8(R8::A),
                    // sra
                    0x28 => self.sra_r8(R8::B),
                    0x29 => self.sra_r8(R8::C),
                    0x2A => self.sra_r8(R8::D),
                    0x2B => self.sra_r8(R8::E),
                    0x2C => self.sra_r8(R8::H),
                    0x2D => self.sra_r8(R8::L),
                    0x2E => self.sra_ref_hl(),
                    0x2F => self.sra_r8(R8::A),
                    // swap
                    0x30 => self.swap_r8(R8::B),
                    0x31 => self.swap_r8(R8::C),
                    0x32 => self.swap_r8(R8::D),
                    0x33 => self.swap_r8(R8::E),
                    0x34 => self.swap_r8(R8::H),
                    0x35 => self.swap_r8(R8::L),
                    0x36 => self.swap_ref_hl(),
                    0x37 => self.swap_r8(R8::A),
                    // srl
                    0x38 => self.srl_r8(R8::B),
                    0x39 => self.srl_r8(R8::C),
                    0x3A => self.srl_r8(R8::D),
                    0x3B => self.srl_r8(R8::E),
                    0x3C => self.srl_r8(R8::H),
                    0x3D => self.srl_r8(R8::L),
                    0x3E => self.srl_ref_hl(),
                    0x3F => self.srl_r8(R8::A),
                    // bit
                    0x40 => self.bit_u3_r8(0, R8::B),
                    0x41 => self.bit_u3_r8(0, R8::C),
                    0x42 => self.bit_u3_r8(0, R8::D),
                    0x43 => self.bit_u3_r8(0, R8::E),
                    0x44 => self.bit_u3_r8(0, R8::H),
                    0x45 => self.bit_u3_r8(0, R8::L),
                    0x46 => self.bit_u3_ref_hl(0),
                    0x47 => self.bit_u3_r8(0, R8::A),
                    0x48 => self.bit_u3_r8(1, R8::B),
                    0x49 => self.bit_u3_r8(1, R8::C),
                    0x4A => self.bit_u3_r8(1, R8::D),
                    0x4B => self.bit_u3_r8(1, R8::E),
                    0x4C => self.bit_u3_r8(1, R8::H),
                    0x4D => self.bit_u3_r8(1, R8::L),
                    0x4E => self.bit_u3_ref_hl(1),
                    0x4F => self.bit_u3_r8(1, R8::A),
                    0x50 => self.bit_u3_r8(2, R8::B),
                    0x51 => self.bit_u3_r8(2, R8::C),
                    0x52 => self.bit_u3_r8(2, R8::D),
                    0x53 => self.bit_u3_r8(2, R8::E),
                    0x54 => self.bit_u3_r8(2, R8::H),
                    0x55 => self.bit_u3_r8(2, R8::L),
                    0x56 => self.bit_u3_ref_hl(2),
                    0x57 => self.bit_u3_r8(2, R8::A),
                    0x58 => self.bit_u3_r8(3, R8::B),
                    0x59 => self.bit_u3_r8(3, R8::C),
                    0x5A => self.bit_u3_r8(3, R8::D),
                    0x5B => self.bit_u3_r8(3, R8::E),
                    0x5C => self.bit_u3_r8(3, R8::H),
                    0x5D => self.bit_u3_r8(3, R8::L),
                    0x5E => self.bit_u3_ref_hl(3),
                    0x5F => self.bit_u3_r8(3, R8::A),
                    0x60 => self.bit_u3_r8(4, R8::B),
                    0x61 => self.bit_u3_r8(4, R8::C),
                    0x62 => self.bit_u3_r8(4, R8::D),
                    0x63 => self.bit_u3_r8(4, R8::E),
                    0x64 => self.bit_u3_r8(4, R8::H),
                    0x65 => self.bit_u3_r8(4, R8::L),
                    0x66 => self.bit_u3_ref_hl(4),
                    0x67 => self.bit_u3_r8(4, R8::A),
                    0x68 => self.bit_u3_r8(5, R8::B),
                    0x69 => self.bit_u3_r8(5, R8::C),
                    0x6A => self.bit_u3_r8(5, R8::D),
                    0x6B => self.bit_u3_r8(5, R8::E),
                    0x6C => self.bit_u3_r8(5, R8::H),
                    0x6D => self.bit_u3_r8(5, R8::L),
                    0x6E => self.bit_u3_ref_hl(5),
                    0x6F => self.bit_u3_r8(5, R8::A),
                    0x70 => self.bit_u3_r8(6, R8::B),
                    0x71 => self.bit_u3_r8(6, R8::C),
                    0x72 => self.bit_u3_r8(6, R8::D),
                    0x73 => self.bit_u3_r8(6, R8::E),
                    0x74 => self.bit_u3_r8(6, R8::H),
                    0x75 => self.bit_u3_r8(6, R8::L),
                    0x76 => self.bit_u3_ref_hl(6),
                    0x77 => self.bit_u3_r8(6, R8::A),
                    0x78 => self.bit_u3_r8(7, R8::B),
                    0x79 => self.bit_u3_r8(7, R8::C),
                    0x7A => self.bit_u3_r8(7, R8::D),
                    0x7B => self.bit_u3_r8(7, R8::E),
                    0x7C => self.bit_u3_r8(7, R8::H),
                    0x7D => self.bit_u3_r8(7, R8::L),
                    0x7E => self.bit_u3_ref_hl(7),
                    0x7F => self.bit_u3_r8(7, R8::A),

                    // res
                    0x80 => self.res_u3_r8(0, R8::B),
                    0x81 => self.res_u3_r8(0, R8::C),
                    0x82 => self.res_u3_r8(0, R8::D),
                    0x83 => self.res_u3_r8(0, R8::E),
                    0x84 => self.res_u3_r8(0, R8::H),
                    0x85 => self.res_u3_r8(0, R8::L),
                    0x86 => self.res_u3_ref_hl(0),
                    0x87 => self.res_u3_r8(0, R8::A),
                    0x88 => self.res_u3_r8(1, R8::B),
                    0x89 => self.res_u3_r8(1, R8::C),
                    0x8A => self.res_u3_r8(1, R8::D),
                    0x8B => self.res_u3_r8(1, R8::E),
                    0x8C => self.res_u3_r8(1, R8::H),
                    0x8D => self.res_u3_r8(1, R8::L),
                    0x8E => self.res_u3_ref_hl(1),
                    0x8F => self.res_u3_r8(1, R8::A),
                    0x90 => self.res_u3_r8(2, R8::B),
                    0x91 => self.res_u3_r8(2, R8::C),
                    0x92 => self.res_u3_r8(2, R8::D),
                    0x93 => self.res_u3_r8(2, R8::E),
                    0x94 => self.res_u3_r8(2, R8::H),
                    0x95 => self.res_u3_r8(2, R8::L),
                    0x96 => self.res_u3_ref_hl(2),
                    0x97 => self.res_u3_r8(2, R8::A),
                    0x98 => self.res_u3_r8(3, R8::B),
                    0x99 => self.res_u3_r8(3, R8::C),
                    0x9A => self.res_u3_r8(3, R8::D),
                    0x9B => self.res_u3_r8(3, R8::E),
                    0x9C => self.res_u3_r8(3, R8::H),
                    0x9D => self.res_u3_r8(3, R8::L),
                    0x9E => self.res_u3_ref_hl(3),
                    0x9F => self.res_u3_r8(3, R8::A),
                    0xA0 => self.res_u3_r8(4, R8::B),
                    0xA1 => self.res_u3_r8(4, R8::C),
                    0xA2 => self.res_u3_r8(4, R8::D),
                    0xA3 => self.res_u3_r8(4, R8::E),
                    0xA4 => self.res_u3_r8(4, R8::H),
                    0xA5 => self.res_u3_r8(4, R8::L),
                    0xA6 => self.res_u3_ref_hl(4),
                    0xA7 => self.res_u3_r8(4, R8::A),
                    0xA8 => self.res_u3_r8(5, R8::B),
                    0xA9 => self.res_u3_r8(5, R8::C),
                    0xAA => self.res_u3_r8(5, R8::D),
                    0xAB => self.res_u3_r8(5, R8::E),
                    0xAC => self.res_u3_r8(5, R8::H),
                    0xAD => self.res_u3_r8(5, R8::L),
                    0xAE => self.res_u3_ref_hl(5),
                    0xAF => self.res_u3_r8(5, R8::A),
                    0xB0 => self.res_u3_r8(6, R8::B),
                    0xB1 => self.res_u3_r8(6, R8::C),
                    0xB2 => self.res_u3_r8(6, R8::D),
                    0xB3 => self.res_u3_r8(6, R8::E),
                    0xB4 => self.res_u3_r8(6, R8::H),
                    0xB5 => self.res_u3_r8(6, R8::L),
                    0xB6 => self.res_u3_ref_hl(6),
                    0xB7 => self.res_u3_r8(6, R8::A),
                    0xB8 => self.res_u3_r8(7, R8::B),
                    0xB9 => self.res_u3_r8(7, R8::C),
                    0xBA => self.res_u3_r8(7, R8::D),
                    0xBB => self.res_u3_r8(7, R8::E),
                    0xBC => self.res_u3_r8(7, R8::H),
                    0xBD => self.res_u3_r8(7, R8::L),
                    0xBE => self.res_u3_ref_hl(7),
                    0xBF => self.res_u3_r8(7, R8::A),

                    // set
                    0xC0 => self.set_u3_r8(0, R8::B),
                    0xC1 => self.set_u3_r8(0, R8::C),
                    0xC2 => self.set_u3_r8(0, R8::D),
                    0xC3 => self.set_u3_r8(0, R8::E),
                    0xC4 => self.set_u3_r8(0, R8::H),
                    0xC5 => self.set_u3_r8(0, R8::L),
                    0xC6 => self.set_u3_ref_hl(0),
                    0xC7 => self.set_u3_r8(0, R8::A),
                    0xC8 => self.set_u3_r8(1, R8::B),
                    0xC9 => self.set_u3_r8(1, R8::C),
                    0xCA => self.set_u3_r8(1, R8::D),
                    0xCB => self.set_u3_r8(1, R8::E),
                    0xCC => self.set_u3_r8(1, R8::H),
                    0xCD => self.set_u3_r8(1, R8::L),
                    0xCE => self.set_u3_ref_hl(1),
                    0xCF => self.set_u3_r8(1, R8::A),
                    0xD0 => self.set_u3_r8(2, R8::B),
                    0xD1 => self.set_u3_r8(2, R8::C),
                    0xD2 => self.set_u3_r8(2, R8::D),
                    0xD3 => self.set_u3_r8(2, R8::E),
                    0xD4 => self.set_u3_r8(2, R8::H),
                    0xD5 => self.set_u3_r8(2, R8::L),
                    0xD6 => self.set_u3_ref_hl(2),
                    0xD7 => self.set_u3_r8(2, R8::A),
                    0xD8 => self.set_u3_r8(3, R8::B),
                    0xD9 => self.set_u3_r8(3, R8::C),
                    0xDA => self.set_u3_r8(3, R8::D),
                    0xDB => self.set_u3_r8(3, R8::E),
                    0xDC => self.set_u3_r8(3, R8::H),
                    0xDD => self.set_u3_r8(3, R8::L),
                    0xDE => self.set_u3_ref_hl(3),
                    0xDF => self.set_u3_r8(3, R8::A),
                    0xE0 => self.set_u3_r8(4, R8::B),
                    0xE1 => self.set_u3_r8(4, R8::C),
                    0xE2 => self.set_u3_r8(4, R8::D),
                    0xE3 => self.set_u3_r8(4, R8::E),
                    0xE4 => self.set_u3_r8(4, R8::H),
                    0xE5 => self.set_u3_r8(4, R8::L),
                    0xE6 => self.set_u3_ref_hl(4),
                    0xE7 => self.set_u3_r8(4, R8::A),
                    0xE8 => self.set_u3_r8(5, R8::B),
                    0xE9 => self.set_u3_r8(5, R8::C),
                    0xEA => self.set_u3_r8(5, R8::D),
                    0xEB => self.set_u3_r8(5, R8::E),
                    0xEC => self.set_u3_r8(5, R8::H),
                    0xED => self.set_u3_r8(5, R8::L),
                    0xEE => self.set_u3_ref_hl(5),
                    0xEF => self.set_u3_r8(5, R8::A),
                    0xF0 => self.set_u3_r8(6, R8::B),
                    0xF1 => self.set_u3_r8(6, R8::C),
                    0xF2 => self.set_u3_r8(6, R8::D),
                    0xF3 => self.set_u3_r8(6, R8::E),
                    0xF4 => self.set_u3_r8(6, R8::H),
                    0xF5 => self.set_u3_r8(6, R8::L),
                    0xF6 => self.set_u3_ref_hl(6),
                    0xF7 => self.set_u3_r8(6, R8::A),
                    0xF8 => self.set_u3_r8(7, R8::B),
                    0xF9 => self.set_u3_r8(7, R8::C),
                    0xFA => self.set_u3_r8(7, R8::D),
                    0xFB => self.set_u3_r8(7, R8::E),
                    0xFC => self.set_u3_r8(7, R8::H),
                    0xFD => self.set_u3_r8(7, R8::L),
                    0xFE => self.set_u3_ref_hl(7),
                    0xFF => self.set_u3_r8(7, R8::A),
                }
            }

            // --- Jumps/calls ---
            // relative jump
            0x18 => self.jr_e8(),
            0x20 => self.jr_cc_n16(CC::NZ),
            0x30 => self.jr_cc_n16(CC::NC),
            0x28 => self.jr_cc_n16(CC::Z),
            0x38 => self.jr_cc_n16(CC::C),
            // return
            0xC0 => self.ret_cc(CC::NZ),
            0xD0 => self.ret_cc(CC::NC),
            0xC8 => self.ret_cc(CC::Z),
            0xD8 => self.ret_cc(CC::C),
            0xC9 => self.ret(),
            0xD9 => self.reti(),
            // conditional jump to addr
            0xC2 => self.jp_cc_n16(CC::NZ),
            0xD2 => self.jp_cc_n16(CC::NC),
            0xCA => self.jp_cc_n16(CC::Z),
            0xDA => self.jp_cc_n16(CC::C),
            // unconditional jump
            0xC3 => self.jp_n16(),
            0xE9 => self.jp_hl(),
            // call
            0xC4 => self.call_cc_n16(CC::NZ),
            0xD4 => self.call_cc_n16(CC::NC),
            0xCC => self.call_cc_n16(CC::Z),
            0xDC => self.call_cc_n16(CC::C),
            0xCD => self.call_n16(),
            // call address vec
            0xC7 => self.rst_vec(RstVec::X00),
            0xD7 => self.rst_vec(RstVec::X10),
            0xE7 => self.rst_vec(RstVec::X20),
            0xF7 => self.rst_vec(RstVec::X30),
            0xCF => self.rst_vec(RstVec::X08),
            0xDF => self.rst_vec(RstVec::X18),
            0xEF => self.rst_vec(RstVec::X28),
            0xFF => self.rst_vec(RstVec::X38),

            // --- 16-bit load instructions ---
            // Load 16 bit register from memory
            0x01 => self.ld_r16_n16(R16::BC),
            0x11 => self.ld_r16_n16(R16::DE),
            0x21 => self.ld_r16_n16(R16::HL),
            0x31 => self.ld_r16_n16(R16::SP),
            // stack pop
            0xC1 => self.pop_r16(R16::BC),
            0xD1 => self.pop_r16(R16::DE),
            0xE1 => self.pop_r16(R16::HL),
            0xF1 => self.pop_r16(R16::AF),
            // stack push
            0xC5 => self.push_r16(R16::BC),
            0xD5 => self.push_r16(R16::DE),
            0xE5 => self.push_r16(R16::HL),
            0xF5 => self.push_r16(R16::AF),
            // misc
            0x08 => self.ld_n16_sp(),
            0xF8 => self.ld_hl_sp_e8(),
            0xF9 => self.ld_sp_hl(),

            // --- 8-bit load instructions ---
            // Write A to memory
            0x02 => self.ld_ref_r16_a(R16::BC),
            0x12 => self.ld_ref_r16_a(R16::DE),
            0x22 => self.ld_ref_hli_a(),
            0x32 => self.ld_ref_hld_a(),
            // Load 8-bit immediate into register
            0x06 => self.ld_r8_n8(R8::B),
            0x16 => self.ld_r8_n8(R8::D),
            0x26 => self.ld_r8_n8(R8::H),
            0x36 => self.ld_ref_hl_n8(),
            0x0E => self.ld_r8_n8(R8::C),
            0x1E => self.ld_r8_n8(R8::E),
            0x2E => self.ld_r8_n8(R8::L),
            0x3E => self.ld_r8_n8(R8::A),
            // Load A from memory
            0x0A => self.ld_a_ref_r16(R16::BC),
            0x1A => self.ld_a_ref_r16(R16::DE),
            0x2A => self.ld_a_ref_hli(),
            0x3A => self.ld_a_ref_hld(),
            // Load into register B
            0x40 => self.ld_r8_r8(R8::B, R8::B),
            0x41 => self.ld_r8_r8(R8::B, R8::C),
            0x42 => self.ld_r8_r8(R8::B, R8::D),
            0x43 => self.ld_r8_r8(R8::B, R8::E),
            0x44 => self.ld_r8_r8(R8::B, R8::H),
            0x45 => self.ld_r8_r8(R8::B, R8::L),
            0x46 => self.ld_r8_ref_hl(R8::B),
            0x47 => self.ld_r8_r8(R8::B, R8::A),
            // Load into register C
            0x48 => self.ld_r8_r8(R8::C, R8::B),
            0x49 => self.ld_r8_r8(R8::C, R8::C),
            0x4A => self.ld_r8_r8(R8::C, R8::D),
            0x4B => self.ld_r8_r8(R8::C, R8::E),
            0x4C => self.ld_r8_r8(R8::C, R8::H),
            0x4D => self.ld_r8_r8(R8::C, R8::L),
            0x4E => self.ld_r8_ref_hl(R8::C),
            0x4F => self.ld_r8_r8(R8::C, R8::A),
            // Load into register D
            0x50 => self.ld_r8_r8(R8::D, R8::B),
            0x51 => self.ld_r8_r8(R8::D, R8::C),
            0x52 => self.ld_r8_r8(R8::D, R8::D),
            0x53 => self.ld_r8_r8(R8::D, R8::E),
            0x54 => self.ld_r8_r8(R8::D, R8::H),
            0x55 => self.ld_r8_r8(R8::D, R8::L),
            0x56 => self.ld_r8_ref_hl(R8::D),
            0x57 => self.ld_r8_r8(R8::D, R8::A),
            // Load into register E
            0x58 => self.ld_r8_r8(R8::E, R8::B),
            0x59 => self.ld_r8_r8(R8::E, R8::C),
            0x5A => self.ld_r8_r8(R8::E, R8::D),
            0x5B => self.ld_r8_r8(R8::E, R8::E),
            0x5C => self.ld_r8_r8(R8::E, R8::H),
            0x5D => self.ld_r8_r8(R8::E, R8::L),
            0x5E => self.ld_r8_ref_hl(R8::E),
            0x5F => self.ld_r8_r8(R8::E, R8::A),
            // Load into register H
            0x60 => self.ld_r8_r8(R8::H, R8::B),
            0x61 => self.ld_r8_r8(R8::H, R8::C),
            0x62 => self.ld_r8_r8(R8::H, R8::D),
            0x63 => self.ld_r8_r8(R8::H, R8::E),
            0x64 => self.ld_r8_r8(R8::H, R8::H),
            0x65 => self.ld_r8_r8(R8::H, R8::L),
            0x66 => self.ld_r8_ref_hl(R8::H),
            0x67 => self.ld_r8_r8(R8::H, R8::A),
            // Load into register L
            0x68 => self.ld_r8_r8(R8::L, R8::B),
            0x69 => self.ld_r8_r8(R8::L, R8::C),
            0x6A => self.ld_r8_r8(R8::L, R8::D),
            0x6B => self.ld_r8_r8(R8::L, R8::E),
            0x6C => self.ld_r8_r8(R8::L, R8::H),
            0x6D => self.ld_r8_r8(R8::L, R8::L),
            0x6E => self.ld_r8_ref_hl(R8::L),
            0x6F => self.ld_r8_r8(R8::L, R8::A),
            // Load into register A
            0x78 => self.ld_r8_r8(R8::A, R8::B),
            0x79 => self.ld_r8_r8(R8::A, R8::C),
            0x7A => self.ld_r8_r8(R8::A, R8::D),
            0x7B => self.ld_r8_r8(R8::A, R8::E),
            0x7C => self.ld_r8_r8(R8::A, R8::H),
            0x7D => self.ld_r8_r8(R8::A, R8::L),
            0x7E => self.ld_r8_ref_hl(R8::A),
            0x7F => self.ld_r8_r8(R8::A, R8::A),
            // Load into [HL]
            0x70 => self.ld_ref_hl_r8(R8::B),
            0x71 => self.ld_ref_hl_r8(R8::C),
            0x72 => self.ld_ref_hl_r8(R8::D),
            0x73 => self.ld_ref_hl_r8(R8::E),
            0x74 => self.ld_ref_hl_r8(R8::H),
            0x75 => self.ld_ref_hl_r8(R8::L),
            0x77 => self.ld_ref_hl_r8(R8::A),
            // special loads
            0xE0 => self.ldh_ref_a8_a(),
            0xF0 => self.ldh_a_ref_a8(),
            0xE2 => self.ldh_ref_c_a(),
            0xF2 => self.ldh_a_ref_c(),
            0xEA => self.ld_ref_n16_a(),
            0xFA => self.ld_a_ref_n16(),

            // --- 16-bit arithmetic/logical instructions ---
            // increment
            0x03 => self.inc_r16(R16::BC),
            0x13 => self.inc_r16(R16::DE),
            0x23 => self.inc_r16(R16::HL),
            0x33 => self.inc_r16(R16::SP),
            // decrement
            0x0B => self.dec_r16(R16::BC),
            0x1B => self.dec_r16(R16::DE),
            0x2B => self.dec_r16(R16::HL),
            0x3B => self.dec_r16(R16::SP),
            // adds to HL
            0x09 => self.add_hl_r16(R16::BC),
            0x19 => self.add_hl_r16(R16::DE),
            0x29 => self.add_hl_r16(R16::HL),
            0x39 => self.add_hl_r16(R16::SP),
            // add to sp
            0xE8 => self.add_sp_e8(),

            // --- 8-bit arithmetic/logical instructions ---
            // increment
            0x04 => self.inc_r8(R8::B),
            0x14 => self.inc_r8(R8::D),
            0x24 => self.inc_r8(R8::H),
            0x34 => self.inc_ref_hl(),
            0x0C => self.inc_r8(R8::C),
            0x1C => self.inc_r8(R8::E),
            0x2C => self.inc_r8(R8::L),
            0x3C => self.inc_r8(R8::A),
            // decrement
            0x05 => self.dec_r8(R8::B),
            0x15 => self.dec_r8(R8::D),
            0x25 => self.dec_r8(R8::H),
            0x35 => self.dec_ref_hl(),
            0x0D => self.dec_r8(R8::C),
            0x1D => self.dec_r8(R8::E),
            0x2D => self.dec_r8(R8::L),
            0x3D => self.dec_r8(R8::A),
            // add
            0x80 => self.add_a_r8(R8::B),
            0x81 => self.add_a_r8(R8::C),
            0x82 => self.add_a_r8(R8::D),
            0x83 => self.add_a_r8(R8::E),
            0x84 => self.add_a_r8(R8::H),
            0x85 => self.add_a_r8(R8::L),
            0x86 => self.add_a_ref_hl(),
            0x87 => self.add_a_r8(R8::A),
            // adc
            0x88 => self.adc_a_r8(R8::B),
            0x89 => self.adc_a_r8(R8::C),
            0x8A => self.adc_a_r8(R8::D),
            0x8B => self.adc_a_r8(R8::E),
            0x8C => self.adc_a_r8(R8::H),
            0x8D => self.adc_a_r8(R8::L),
            0x8E => self.adc_a_ref_hl(),
            0x8F => self.adc_a_r8(R8::A),
            // sub
            0x90 => self.sub_a_r8(R8::B),
            0x91 => self.sub_a_r8(R8::C),
            0x92 => self.sub_a_r8(R8::D),
            0x93 => self.sub_a_r8(R8::E),
            0x94 => self.sub_a_r8(R8::H),
            0x95 => self.sub_a_r8(R8::L),
            0x96 => self.sub_a_ref_hl(),
            0x97 => self.sub_a_r8(R8::A),
            // sbc
            0x98 => self.sbc_a_r8(R8::B),
            0x99 => self.sbc_a_r8(R8::C),
            0x9A => self.sbc_a_r8(R8::D),
            0x9B => self.sbc_a_r8(R8::E),
            0x9C => self.sbc_a_r8(R8::H),
            0x9D => self.sbc_a_r8(R8::L),
            0x9E => self.sbc_a_ref_hl(),
            0x9F => self.sbc_a_r8(R8::A),
            // and
            0xA0 => self.and_a_r8(R8::B),
            0xA1 => self.and_a_r8(R8::C),
            0xA2 => self.and_a_r8(R8::D),
            0xA3 => self.and_a_r8(R8::E),
            0xA4 => self.and_a_r8(R8::H),
            0xA5 => self.and_a_r8(R8::L),
            0xA6 => self.and_a_ref_hl(),
            0xA7 => self.and_a_r8(R8::A),
            // xor
            0xA8 => self.xor_a_r8(R8::B),
            0xA9 => self.xor_a_r8(R8::C),
            0xAA => self.xor_a_r8(R8::D),
            0xAB => self.xor_a_r8(R8::E),
            0xAC => self.xor_a_r8(R8::H),
            0xAD => self.xor_a_r8(R8::L),
            0xAE => self.xor_a_ref_hl(),
            0xAF => self.xor_a_r8(R8::A),
            // or
            0xB0 => self.or_a_r8(R8::B),
            0xB1 => self.or_a_r8(R8::C),
            0xB2 => self.or_a_r8(R8::D),
            0xB3 => self.or_a_r8(R8::E),
            0xB4 => self.or_a_r8(R8::H),
            0xB5 => self.or_a_r8(R8::L),
            0xB6 => self.or_a_ref_hl(),
            0xB7 => self.or_a_r8(R8::A),
            // cp
            0xB8 => self.cp_a_r8(R8::B),
            0xB9 => self.cp_a_r8(R8::C),
            0xBA => self.cp_a_r8(R8::D),
            0xBB => self.cp_a_r8(R8::E),
            0xBC => self.cp_a_r8(R8::H),
            0xBD => self.cp_a_r8(R8::L),
            0xBE => self.cp_a_ref_hl(),
            0xBF => self.cp_a_r8(R8::A),
            // Operations with immediate operand
            0xC6 => self.add_a_n8(),
            0xD6 => self.sub_a_n8(),
            0xE6 => self.and_a_n8(),
            0xF6 => self.or_a_n8(),
            0xCE => self.adc_a_n8(),
            0xDE => self.sbc_a_n8(),
            0xEE => self.xor_a_n8(),
            0xFE => self.cp_a_n8(),

            // --- 8-bit shift, rotate and bit instructions ---
            // rotate accumulator register
            0x07 => self.rlca(),
            0x17 => self.rla(),
            0x0F => self.rrca(),
            0x1F => self.rra(),

            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                panic!("Instruction {opcode:X} is not supported on the game boy")
            }
        }
    }
}

#[cfg(test)]
mod test {
    use std::io::BufWriter;

    use super::Cpu;

    #[ignore]
    #[test]
    fn run_boot_rom() {
        let boot_rom = include_bytes!("../roms/dmg_boot.bin");
        let mut cpu = Cpu::create(boot_rom);
        while cpu.regs.pc != 0x100 {
            cpu.step();
        }
    }

    // #[ignore]
    #[test]
    fn test_debug_rom() {
        let file = std::fs::File::create("out.txt").unwrap();
        eprintln!("Writing to file: {:?}", file);
        let file = BufWriter::with_capacity(1, file);
        // let file = BufWriter::new(file);
        let test_rom =
            include_bytes!("../roms/gb-test-roms/cpu_instrs/individual/02-interrupts.gb");
        // include_bytes!("../roms/gb-test-roms/cpu_instrs/individual/03-op sp,hl.gb");
        // include_bytes!("../roms/gb-test-roms/cpu_instrs/individual/11-op a,(hl).gb");
        let mut cpu = Cpu::_debug_mode(test_rom, file);
        let mut line = 1;
        loop {
            println!("L: {}", line);
            line += 1;
            cpu.step();
            // if line == 152040 {
            //     break;
            // }
        }
    }
}
