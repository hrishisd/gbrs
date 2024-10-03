#![allow(unused)]

use std::{cmp::min, slice};
pub struct Mmu {
    rom_bank_0: [u8; 0x4000],
    rom_bank_n: [u8; 0x4000],
    ext_ram: [u8; 0x2000],
    work_ram: [u8; 0x2000],
    zero_page: [u8; 0x80],
}

impl Mmu {
    pub fn create(rom: &[u8]) -> Self {
        let mut rom_bank_0 = [0; 0x4000];
        for idx in 0..rom_bank_0.len().min(rom.len()) {
            rom_bank_0[idx] = rom[idx];
        }
        // TODO: initialize other rom banks
        let rom_bank_n = [0; 0x4000];
        Mmu {
            rom_bank_0,
            rom_bank_n,
            ext_ram: [0; 0x2000],
            work_ram: [0; 0x2000],
            zero_page: [0; 0x80],
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
                // TODO
                // addr & 0x1FFF
                2
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
            0xFEA0..=0xFEFF => 0,
            // io registers
            0xFF00..=0xFF7F => {
                todo!()
            }
            // high ram?
            0xFF80..=0xFFFE => {
                todo!()
            }
            // interrupt enable register
            0xFFFF => {
                todo!()
            }
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
            0xFF00..=0xFF7F => {
                todo!()
            }
            // high ram?
            0xFF80..=0xFFFE => {
                todo!()
            }
            // interrupt enable register
            0xFFFF => {
                todo!()
            }
        }
    }

    pub fn write_word(&mut self, addr: u16, word: u16) {
        let [lo, hi] = word.to_le_bytes();
        self.write_byte(addr, lo);
        self.write_byte(addr + 1, hi);
    }
}
