#![allow(unused)]
pub struct Mmu {}

impl Mmu {
    pub fn create() -> Self {
        Mmu {}
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        todo!()
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        todo!()
    }

    pub fn write_byte(&self, addr: u16, byte: u8) {
        todo!()
    }

    pub fn write_word(&self, addr: u16, word: u16) {
        todo!()
    }
}
