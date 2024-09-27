use std::net::TcpListener;

use register_file::{Flag, Registers, R16, R8};

use crate::mmu::MMU;

mod opcode;
mod register_file;
struct CPU {
    regs: Registers,
    mmu: MMU,
}

impl CPU {
    /// Fetch, decode, and execute a single instruction
    /// Returns the number of master clock cycles (at 4 MHz) that the instruction takes.
    /// E.g. the executing `NOP` instruction will return 4
    fn step(&mut self) -> u8 {
        let opcode = self.mmu.read_byte(self.regs.pc);
        self.regs.pc += 1;
        let t_cycles = self.execute(opcode);
        assert!(t_cycles % 4 == 0 && t_cycles < 24, "Unexpected number of t-cycles during execution of opcode {opcode:x} execution: {t_cycles}");
        t_cycles
    }

    /// Execute a single instruction and return the number of system clock ticks (T-cycles) the instruction takes
    ///
    /// ref: https://gbdev.io/gb-opcodes//optables/
    fn execute(&mut self, opcode: u8) -> u8 {
        use register_file::R16::{AF, BC, DE, HL};
        match opcode {
            // NOOP
            0x00 => 4,
            // Stop
            0x10 => {
                // The opcode of this instruction is $10, but it has to be followed by an additional byte that is ignored by the CPU (any value works, but normally $00 is used).
                // TODO put CPU in low power mode and switch between double and normal speed CPU omdes in GBC.
                self.regs.pc += 1;
                4
            }
            // Load 16 bit register from memory
            0x01 => {
                let word = self.mmu.read_word(self.regs.pc);
                self.regs.set_bc(word);
                self.regs.pc += 2;
                12
            }
            0x11 => {
                let word = self.mmu.read_word(self.regs.pc);
                self.regs.set_de(word);
                self.regs.pc += 2;
                12
            }
            0x21 => {
                let word = self.mmu.read_word(self.regs.pc);
                self.regs.set_hl(word);
                self.regs.pc += 2;
                12
            }
            0x31 => {
                let word = self.mmu.read_word(self.regs.pc);
                self.regs.sp = word;
                self.regs.pc += 2;
                12
            }
            // Write A to memory
            0x02 => {
                let addr = self.regs.bc();
                self.mmu.write_byte(addr, self.regs.a);
                8
            }
            0x12 => {
                let addr = self.regs.de();
                self.mmu.write_byte(addr, self.regs.a);
                8
            }
            0x22 => {
                let addr = self.regs.hl();
                self.regs.set_hl(addr + 1);
                self.mmu.write_byte(addr, self.regs.a);
                8
            }
            0x32 => {
                let addr = self.regs.hl();
                self.regs.set_hl(addr - 1);
                self.mmu.write_byte(addr, self.regs.a);
                8
            }
            // Increment 16-bit registers
            0x03 => {
                self.regs.set_bc(self.regs.bc() + 1);
                8
            }
            0x13 => {
                self.regs.set_de(self.regs.de() + 1);
                8
            }
            0x23 => {
                self.regs.set_hl(self.regs.hl() + 1);
                8
            }
            0x33 => {
                self.regs.sp += 1;
                8
            }
            0xD3 | 0xDB | 0xDD | 0xE3 | 0xE4 | 0xEB | 0xEC | 0xED | 0xF4 | 0xFC | 0xFD => {
                panic!("Instruction {opcode:X} is not supported on the game boy")
            }
            other => {
                todo!("Unimplemented: {other:x}")
            }
        }
    }
}
