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
            rom.len() == 0x8000,
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

pub struct Mbc1 {
    rom_banks: Vec<[u8; 0x4000]>,
    rom_bank_idx: usize,
    ram_banks: Vec<[u8; 0x2000]>,
    ram_bank_idx: usize,
    ram_enable: bool,
    _bank_mode_select: BankModeSelect,
}

#[allow(dead_code)]
enum BankModeSelect {
    ExtendedROMBanking,
    RAMBanking,
}

impl Mbc1 {
    pub fn from_game_rom(rom: &[u8]) -> Self {
        let rom_size_byte = rom[0x0148];
        assert!((0x00..=0x08).contains(&rom_size_byte));
        let num_banks = 2 * (1 << rom_size_byte);
        let mut rom_banks = vec![[0; 0x4000]; num_banks];
        assert_eq!(
            rom.len(),
            num_banks * 1 << 14,
            "ROM should be num banks * 16 KiB"
        );
        for idx in 0..rom_banks.len() {
            let bank_size = 0x4000;
            rom_banks[idx].copy_from_slice(&rom[idx * bank_size..((idx + 1) * bank_size)]);
        }

        let ram_size_byte = rom[0x0149];
        let ram_banks = match ram_size_byte {
            0x00 | 0x01 => {
                vec![]
            }
            0x02 => {
                vec![[0u8; 0x2000]; 1]
            }
            0x03 => {
                vec![[0u8; 0x2000]; 4]
            }
            _ => {
                panic!("Unexpected RAM size for MBC 1: {:X}", ram_size_byte)
            }
        };
        assert!((0x00..=0x08).contains(&rom_size_byte));
        Mbc1 {
            rom_banks,
            ram_banks,
            rom_bank_idx: 0,
            ram_bank_idx: 0,
            ram_enable: false,
            _bank_mode_select: BankModeSelect::RAMBanking,
        }
    }
}

impl Cartridge for Mbc1 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom_banks[0][addr as usize],
            0x4000..=0x7FFF => {
                let bank_idx = std::cmp::min(1, self.rom_bank_idx);
                self.rom_banks[bank_idx][(addr - 0x4000) as usize]
            }
            0xA000..=0xBFFF => {
                if self.ram_enable {
                    self.ram_banks[self.ram_bank_idx][addr as usize - 0xA000]
                } else {
                    0xFF
                }
            }

            _ => panic!("invalid cartridge read: {}", addr),
        }
    }

    fn write(&mut self, addr: u16, byte: u8) {
        match addr {
            0x0000..=0x1FFF => {
                if addr & 0x0F == 0x0A {
                    self.ram_enable = true;
                } else {
                    self.ram_enable = false;
                }
            }
            0x2000..=0x3FFF => {
                // TODO: maybe mask this further if idx out of bounds error
                let idx = byte & 0b0001_1111;
                self.rom_bank_idx = idx as usize;
            }
            0x4000..=0x5FFF => {
                let idx = byte & 0b0011;
                self.ram_bank_idx = idx as usize;
            }
            0x6000..=0x7FFF => {
                // TODO: bank mode select
                panic!("Have not implemented bank mode select for MBC1")
            }
            0xA000..=0xBFFF => {
                if self.ram_enable {
                    self.ram_banks[self.ram_bank_idx][addr as usize - 0xA000] = byte;
                }
            }
            _ => panic!("Illegal write to cartridge"),
        }
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
