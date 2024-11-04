pub trait Cartridge {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, byte: u8);
}

/// Small games of not more than 32 KiB ROM do not require a MBC chip for ROM banking.
/// The ROM is directly mapped to memory at $0000-7FFF.
/// Optionally up to 8 KiB of RAM could be connected at $A000-BFFF.
pub struct NoMbc {
    rom: [u8; 0x8000],
    ext_ram: [u8; 0x2000],
}

impl NoMbc {
    pub fn from_game_rom(rom: &[u8]) -> Self {
        assert!(
            rom.len() <= 0x8000,
            "Cartridge with No MBC only supports 32 KiB ROM"
        );
        let mut cart_rom = [0; 0x8000];
        cart_rom[..rom.len()].copy_from_slice(rom);
        NoMbc {
            rom: cart_rom,
            ext_ram: [0; 0x2000],
        }
    }
}

impl Cartridge for NoMbc {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x7FFF => self.rom[addr as usize],
            0xA000..=0xBFFF => self.ext_ram[addr as usize - 0xA000],
            _ => panic!("Invalid cartridge memory access: {:0X}", addr),
        }
    }

    fn write(&mut self, addr: u16, byte: u8) {
        match addr {
            0x0000..=0x7FFF => eprintln!("attempted to write to ROM, {addr:0X} <- {byte:0X}"),
            0xA000..=0xBFFF => self.ext_ram[addr as usize - 0xA000] = byte,
            _ => panic!("Invalid cartridge memory access: {:0X}", addr),
        }
    }
}

pub struct Mbc1 {}

impl Mbc1 {
    pub fn from_game_rom(_rom: &[u8]) -> Self {
        todo!("Implement MBC 1")
    }
}

impl Cartridge for Mbc1 {
    fn read(&self, _addr: u16) -> u8 {
        todo!("Implement MBC1 read")
    }

    fn write(&mut self, _addr: u16, _byte: u8) {
        todo!("Implement MBC1 write")
    }
}

pub struct Mbc3 {}

impl Mbc3 {
    pub fn from_game_rom(_rom: &[u8]) -> Self {
        todo!("Implement MBC 3")
    }
}

impl Cartridge for Mbc3 {
    fn read(&self, _addr: u16) -> u8 {
        todo!("Implement MBC 3 read")
    }

    fn write(&mut self, _addr: u16, _byte: u8) {
        todo!("Implement MBC 3 write")
    }
}
