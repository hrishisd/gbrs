#![allow(unused)]

use crate::timer::{Timer, TimerFrequency};
use crate::util::U8Ext;
use core::panic;
use std::{cmp::min, slice};
pub struct Mmu {
    rom_bank_0: [u8; 0x4000],
    rom_bank_n: [u8; 0x4000],
    ext_ram: [u8; 0x2000],
    work_ram: [u8; 0x2000],
    high_ram: [u8; 0x80],

    // TODO: replace this with a GPU implementation
    vram: [u8; 0x0200],
    /// A set of flags that indicates whether the interrupt handler for each corresponding piece of hardware may be called.
    ///
    /// also referred to as `IE`
    interrupt_enable_flags: InterruptFlags,
    /// TODO: handle timer interrupts
    timer: Timer,
    /// TODO: reset when executing STOP instruction and only begin ticking once stop mode ends
    divider: Timer,
}

impl Mmu {
    pub fn create(rom: &[u8]) -> Self {
        let mut rom_bank_0 = [0; 0x4000];
        for idx in 0..rom_bank_0.len().min(rom.len()) {
            rom_bank_0[idx] = rom[idx];
        }
        // TODO: initialize other rom banks
        let mut rom_bank_n = [0; 0x4000];
        if rom.len() > 0x4000 {
            for idx in 0..rom_bank_n.len() {
                let rom_idx = idx + 0x4000;
                if rom_idx > rom.len() {
                    break;
                }
                rom_bank_n[idx] = rom[rom_idx];
            }
        }
        Mmu {
            rom_bank_0,
            rom_bank_n,
            ext_ram: [0; 0x2000],
            work_ram: [0; 0x2000],
            high_ram: [0; 0x80],
            vram: [0; 0x0200],
            interrupt_enable_flags: InterruptFlags::from_byte(0x00),
            timer: Timer::new(TimerFrequency::F4KiHz),
            divider: Timer::new(TimerFrequency::F16KiHz),
        }
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM bank 0
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize],
            // ROM bank 01-NN
            0x4000..=0x7FFF => self.rom_bank_n[(addr & 0x3FFF) as usize],
            // VRAM
            0x8000..=0x9FFF => {
                // TODO: replace this with gpu implementation
                self.vram[addr as usize - 0x8000]
            }
            // external RAM
            0xA000..=0xBFFF => self.ext_ram[(addr & 0x1FFF) as usize],
            // work RAM
            0xC000..=0xDFFF => self.work_ram[(addr & 0x1FFF) as usize],
            // echo RAM
            0xE000..=0xFDFF => self.work_ram[(addr & 0x1FFF) as usize],
            // object attribute memory
            0xFE00..=0xFE9F => 6,
            // not usable
            0xFEA0..=0xFEFF => {
                panic!("Program accessed invalid memory: {addr:X}")
            }
            // io registers
            0xFF00..=0xFF7F => {
                // TODO: implement
                return match addr {
                    0xFF00 => {
                        // joypad input
                        0
                    }
                    0xFF01 | 0xFF02 => 0, // TODO: serial
                    0xFF04..=0xFF07 => {
                        // TODO: timer and divider
                        0
                    }
                    // TODO: Interrupt flag
                    0xFF0F => 0,
                    0xFF10..=0xFF26 => {
                        // TODO: audio
                        0
                    }
                    0xFF30..=0xFF3F => {
                        // TODO: wave pattern
                        0
                    }
                    0xFF40..=0xFF4B => {
                        // TODO: LCD constrol, status, position, scrolling, and palettes
                        0
                    }
                    0xFF4F => {
                        // VRAM bank select
                        0
                    }
                    0xFF50 => {
                        // set to non-zero to disable boot ROM
                        todo!("unmap boot ROM")
                    }
                    0xFF51..=0xFF55 => {
                        // VRAM DMA
                        0
                    }
                    0xF680..=0xFF6B => {
                        // BG / OBJ palettes
                        0
                    }
                    0xFF70 => {
                        // WRAM bank select
                        0
                    }
                    _ => panic!("BUG: unhandled IO register read for addr: {addr:X}"),
                };
            }
            // high ram?
            0xFF80..=0xFFFE => self.high_ram[addr as usize - 0xFF80],
            // interrupt enable register
            0xFFFF => self.interrupt_enable_flags.as_byte(),
        }
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr);
        let hi = self.read_byte(addr + 1);
        u16::from_le_bytes([lo, hi])
    }

    pub fn write_byte(&mut self, addr: u16, byte: u8) {
        match addr {
            // ROM bank 0
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize] = byte,
            // ROM bank 01-NN
            0x4000..=0x7FFF => self.rom_bank_n[(addr & 0x3FFF) as usize] = byte,
            // VRAM
            0x8000..=0x9FFF => {
                // TODO
                // addr & 0x1FFF
                self.vram[addr as usize - 0x8000] = byte;
            }
            // external RAM
            0xA000..=0xBFFF => self.ext_ram[(addr & 0x1FFF) as usize] = byte,
            // work RAM
            0xC000..=0xDFFF => self.work_ram[(addr & 0x1FFF) as usize] = byte,
            // echo RAM
            0xE000..=0xFDFF => self.work_ram[(addr & 0x1FFF) as usize] = byte,
            // object attribute memory
            0xFE00..=0xFE9F => {}
            // not usable
            0xFEA0..=0xFEFF => {}
            // io registers
            0xFF00..=0xFF7F => match addr {
                0xFF00 => {
                    // joypad input
                    todo!()
                }
                0xFF01 | 0xFF02 => {
                    // serial transfer
                    todo!()
                }
                0xFF04 => {
                    self.divider.value = 0;
                }
                0xFF05 => {
                    self.timer.value = byte;
                }
                0xFF06 => {
                    self.timer.tma = byte;
                }
                0xFF07 => {
                    // TAC timer control
                    // byte.as_bits()
                    let [.., enable, clock_select_1, clock_select_0] = byte.bits();
                    let frequency = match [clock_select_1, clock_select_0] {
                        [false, false] => TimerFrequency::F4KiHz,
                        [false, true] => TimerFrequency::F16KiHz,
                        [true, false] => TimerFrequency::F64KiHz,
                        [true, true] => TimerFrequency::F256KiHz,
                    };
                    self.timer.enabled = enable;
                    self.timer.frequency = frequency;
                }
                0xFF0F => {
                    self.interrupt_enable_flags = InterruptFlags::from_byte(byte);
                }
                0xFF10..=0xFF26 => {
                    // TODO: implement audio
                }
                0xFF30..=0xFF3F => {
                    // wave pattern
                    todo!();
                }
                0xFF40..=0xFF4B => {
                    // LCD control, status, position, scrolling, and palettes
                    todo!();
                }
                0xFF4F => {
                    // VRAM bank select
                    todo!()
                }
                0xFF50 => {
                    // set to non-zero to disable boot ROM
                    todo!()
                }
                0xFF51..=0xFF55 => {
                    // VRAM DMA
                    todo!()
                }
                0xF680..=0xFF6B => {
                    // BG / OBJ palettes
                    todo!();
                }
                0xFF70 => {
                    // WRAM bank select
                    todo!();
                }
                _ => panic!("BUG: unhandled IO register read for addr: {addr:X}"),
            },
            // high ram, used by LDH instructions
            0xFF80..=0xFFFE => {
                self.high_ram[addr as usize - 0xFF80] = byte;
            }
            // interrupt enable register
            0xFFFF => self.interrupt_enable_flags = InterruptFlags::from_byte(byte),
        }
    }

    pub fn write_word(&mut self, addr: u16, word: u16) {
        let [lo, hi] = word.to_le_bytes();
        self.write_byte(addr, lo);
        self.write_byte(addr + 1, hi);
    }
}

/// Flags for each interrupt handler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct InterruptFlags {
    joypad: bool,
    serial: bool,
    timer: bool,
    lcd: bool,
    vblank: bool,
}

impl InterruptFlags {
    fn from_byte(byte: u8) -> Self {
        let [_, _, _, joypad, serial, timer, lcd, vblank] = byte.bits();
        InterruptFlags {
            joypad,
            serial,
            timer,
            lcd,
            vblank,
        }
    }

    fn as_byte(self) -> u8 {
        (self.joypad as u8) << 4
            | (self.serial as u8) << 3
            | (self.timer as u8) << 2
            | (self.lcd as u8) << 1
            | (self.vblank as u8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn from_byte() {
        let flags = InterruptFlags::from_byte(0b00011111);
        assert!(flags.joypad);
        assert!(flags.serial);
        assert!(flags.timer);
        assert!(flags.lcd);
        assert!(flags.vblank);

        let byte = 0b00011000;
        let flags = InterruptFlags::from_byte(byte);
        assert!(flags.joypad);
        assert!(flags.serial);
        assert_eq!(flags.timer, false);
        assert_eq!(flags.lcd, false);
        assert_eq!(flags.vblank, false);

        assert_eq!(flags.as_byte(), byte);
    }
}
