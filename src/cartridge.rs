use std::time::{Duration, SystemTime};

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_big_array::BigArray;

#[typetag::serde(tag = "cartridge")]
pub trait Cartridge {
    fn read(&self, addr: u16) -> u8;
    fn write(&mut self, addr: u16, byte: u8);
    /// When loading the cartridge state from a save file, use this to set the rom data in the cartridge
    fn set_rom(&mut self, rom: &[u8]);
}

/// Small games of not more than 32 KiB ROM do not require a MBC chip for ROM banking.
/// The ROM is directly mapped to memory at $0000-7FFF.
/// Optionally up to 8 KiB of RAM could be connected at $A000-BFFF.
#[derive(Serialize, Deserialize)]
pub struct NoMbc {
    #[serde(
        serialize_with = "skip_serializing_rom",
        deserialize_with = "create_default_rom"
    )]
    rom: [u8; 0x8000],
    #[serde(with = "BigArray")]
    ext_ram: [u8; 0x2000],
}

fn skip_serializing_rom<S>(_: &[u8; 0x8000], s: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    s.serialize_none()
}

fn create_default_rom<'de, D>(_: D) -> Result<[u8; 0x8000], D::Error>
where
    D: Deserializer<'de>,
{
    Ok([0; 0x8000])
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

#[typetag::serde]
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

    fn set_rom(&mut self, rom: &[u8]) {
        assert_eq!(
            rom.len(),
            self.rom.len(),
            "incorrect ROM length for MBC 1. Expected {}, got {}",
            self.rom.len(),
            rom.len()
        );
        self.rom.copy_from_slice(rom);
    }
}

#[derive(Serialize, Deserialize)]
pub struct Mbc1 {
    #[serde(skip)]
    rom_banks: Vec<RomBank>,
    rom_bank_idx: usize,
    ram_banks: Vec<RamBank>,
    ram_bank_idx: usize,
    ram_enable: bool,
}

fn parse_banks(rom: &[u8]) -> Vec<RomBank> {
    let rom_size_byte = rom[0x0148];
    assert!((0x00..=0x08).contains(&rom_size_byte));
    let num_banks = 2 * (1 << rom_size_byte);
    let mut rom_banks = vec![RomBank([0; 0x4000]); num_banks];
    assert_eq!(
        rom.len(),
        num_banks * (0x4000),
        "ROM should be num banks * 16 KiB"
    );
    for idx in 0..rom_banks.len() {
        let bank_size = 0x4000;
        rom_banks[idx]
            .0
            .copy_from_slice(&rom[idx * bank_size..((idx + 1) * bank_size)]);
    }
    rom_banks
}

impl Mbc1 {
    pub fn from_game_rom(rom: &[u8]) -> Self {
        let rom_banks = parse_banks(rom);
        assert!(
            rom_banks.len() <= 32,
            "Only support 5 bits for ROM bank selection"
        );
        let ram_size_byte = rom[0x0149];
        let ram_banks = match ram_size_byte {
            0x00 | 0x01 => {
                vec![]
            }
            0x02 => {
                vec![RamBank([0u8; 0x2000]); 1]
            }
            0x03 => {
                vec![RamBank([0u8; 0x2000]); 4]
            }
            _ => {
                panic!("Unexpected RAM size for MBC 1: {:X}", ram_size_byte)
            }
        };
        Mbc1 {
            rom_banks,
            ram_banks,
            rom_bank_idx: 1,
            ram_bank_idx: 0,
            ram_enable: false,
        }
    }
}

#[typetag::serde]
impl Cartridge for Mbc1 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom_banks[0].as_slice()[addr as usize],
            0x4000..=0x7FFF => {
                self.rom_banks[self.rom_bank_idx].as_slice()[(addr - 0x4000) as usize]
            }
            0xA000..=0xBFFF => {
                if self.ram_enable {
                    self.ram_banks[self.ram_bank_idx].as_slice()[addr as usize - 0xA000]
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
                self.ram_enable = byte & 0xF == 0xA;
            }
            0x2000..=0x3FFF => {
                // TODO: maybe mask this further if idx out of bounds error
                let idx = byte & 0b0001_1111;
                self.rom_bank_idx = match idx {
                    0 => 1,
                    _ => idx as usize,
                };
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
                    self.ram_banks[self.ram_bank_idx].as_mut_slice()[addr as usize - 0xA000] = byte;
                }
            }
            _ => panic!("Illegal write to cartridge: {} <- {}", addr, byte),
        }
    }

    fn set_rom(&mut self, rom: &[u8]) {
        let banks = parse_banks(rom);
        self.rom_banks = banks;
    }
}

/// Either RAM/clock is disabled, or we have mapped in a ram bank, or we have mapped a clock register.
#[derive(Serialize, Deserialize)]
enum RamBankOrRtcSelect {
    Ram { idx: u8 },
    Seconds,
    Minutes,
    Hours,
    DayCounterLoBits,
    DayCounterHiBits,
}

/// Controls when the clock data is latched to the clock registers
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
enum LatchState {
    Latched,
    Staged,
}

#[derive(Debug, Serialize, Deserialize)]
struct RealTimeClockRegisters {
    seconds: u8,  // 0-59
    minutes: u8,  // 0-59
    hours: u8,    // 0-23
    days_low: u8, // Lower 8 bits of day counter
    days_hi_bit: bool,
    day_counter_carry: bool,
    // We use system time instead of Instant because Instant is opaque and not serializable.
    last_update_time: SystemTime,
}
impl RealTimeClockRegisters {
    fn update(&mut self) {
        let now = SystemTime::now();
        let elapsed = now
            .duration_since(self.last_update_time)
            .unwrap_or(Duration::ZERO)
            .as_secs();
        if elapsed == 0 {
            return;
        }
        self.last_update_time = now;

        // Update seconds, minutes, hours, and days
        let total_seconds = self.seconds as u64 + elapsed;
        self.seconds = (total_seconds % 60) as u8;

        let total_minutes = self.minutes as u64 + (total_seconds / 60);
        self.minutes = (total_minutes % 60) as u8;

        let total_hours = self.hours as u64 + (total_minutes / 60);
        self.hours = (total_hours % 24) as u8;

        let total_days = (if self.days_hi_bit { 256 } else { 0 } + self.days_low as u16) as u64
            + (total_hours / 24);

        // Check for day counter overflow (> 511 days)
        if total_days > 511 {
            self.day_counter_carry = true;
        }

        self.days_low = (total_days % 256) as u8;
        self.days_hi_bit = (total_days % 512) >= 256;
    }
}

#[derive(Serialize, Deserialize)]
pub struct Mbc3 {
    #[serde(skip)]
    rom_banks: Vec<RomBank>,
    rom_bank_idx: usize,
    ram_banks: Vec<RamBank>,
    enable_ram_and_rtc: bool,
    ram_bank_or_rtc_select: RamBankOrRtcSelect,
    clock_registers: RealTimeClockRegisters,
    latch_state: LatchState,
}

impl Mbc3 {
    pub fn from_game_rom(rom: &[u8]) -> Self {
        let rom_size_byte = rom[0x0148];
        assert!(
            (0x00..=0x06).contains(&rom_size_byte),
            "MBC3 can have up to 2 MiB of ROM"
        );
        let num_banks = 2 * (1 << rom_size_byte);
        assert_eq!(
            rom.len(),
            num_banks * (0x4000),
            "ROM should be num banks * 16 KiB"
        );
        let mut rom_banks = vec![RomBank([0; 0x4000]); num_banks];
        for idx in 0..rom_banks.len() {
            let bank_size = 0x4000;
            rom_banks[idx]
                .0
                .copy_from_slice(&rom[idx * bank_size..((idx + 1) * bank_size)]);
        }

        let ram_size_byte = rom[0x0149];
        let ram_banks = match ram_size_byte {
            0x00 | 0x01 => {
                vec![]
            }
            0x02 => {
                vec![RamBank([0u8; 0x2000]); 1]
            }
            0x03 => {
                vec![RamBank([0u8; 0x2000]); 4]
            }
            _ => {
                panic!("Unexpected RAM size for MBC 1: {:X}", ram_size_byte)
            }
        };
        assert!((0x00..=0x08).contains(&rom_size_byte));
        Mbc3 {
            rom_banks,
            rom_bank_idx: 1,
            ram_banks,
            ram_bank_or_rtc_select: RamBankOrRtcSelect::Ram { idx: 0 },
            clock_registers: RealTimeClockRegisters {
                seconds: 0,
                minutes: 0,
                hours: 0,
                days_low: 0,
                days_hi_bit: false,
                day_counter_carry: false,
                last_update_time: SystemTime::now(),
            },
            enable_ram_and_rtc: false,
            latch_state: LatchState::Latched,
        }
    }
}

#[typetag::serde]
impl Cartridge for Mbc3 {
    fn read(&self, addr: u16) -> u8 {
        match addr {
            0x0000..=0x3FFF => self.rom_banks[0].as_slice()[addr as usize],
            0x4000..=0x7FFF => self.rom_banks[self.rom_bank_idx].as_slice()[addr as usize - 0x4000],
            0xA000..=0xBFFF => {
                if self.enable_ram_and_rtc {
                    match self.ram_bank_or_rtc_select {
                        RamBankOrRtcSelect::Ram { idx } => {
                            self.ram_banks[idx as usize].as_slice()[addr as usize - 0xA000]
                        }
                        RamBankOrRtcSelect::Seconds => self.clock_registers.seconds,
                        RamBankOrRtcSelect::Minutes => self.clock_registers.minutes,
                        RamBankOrRtcSelect::Hours => self.clock_registers.hours,
                        RamBankOrRtcSelect::DayCounterLoBits => self.clock_registers.days_low,
                        RamBankOrRtcSelect::DayCounterHiBits => {
                            let mut value = 0;
                            if self.clock_registers.days_hi_bit {
                                value |= 0x01;
                            }
                            if self.clock_registers.day_counter_carry {
                                value |= 0x80;
                            }
                            value
                        }
                    }
                } else {
                    0xFF
                }
            }
            _ => {
                todo!("BUG: Invalid read from mbc3 cartridge")
            }
        }
    }

    fn write(&mut self, addr: u16, byte: u8) {
        match addr {
            0x0000..=0x1FFF => match byte & 0xF {
                0xA => self.enable_ram_and_rtc = true,
                0x0 => self.enable_ram_and_rtc = false,
                _ => {}
            },
            0x2000..=0x3FFF => {
                let rom_bank_number = byte & 0x07F;
                self.rom_bank_idx = if rom_bank_number == 0 {
                    1
                } else {
                    rom_bank_number as usize
                };
            }
            0x4000..=0x5FFF => {
                self.ram_bank_or_rtc_select = match byte {
                    0x0..=0x3 => RamBankOrRtcSelect::Ram { idx: byte },
                    0x8 => RamBankOrRtcSelect::Seconds,
                    0x9 => RamBankOrRtcSelect::Minutes,
                    0xA => RamBankOrRtcSelect::Hours,
                    0xB => RamBankOrRtcSelect::DayCounterLoBits,
                    0xC => RamBankOrRtcSelect::DayCounterHiBits,
                    _ => {
                        // ignore other writes
                        return;
                    }
                };
            }
            0x6000..=0x7FFF => match byte {
                0x0 => self.latch_state = LatchState::Staged,
                0x1 if self.latch_state == LatchState::Staged => {
                    self.clock_registers.update();
                    self.latch_state = LatchState::Latched
                }
                _ => {}
            },
            0xA000..=0xBFFF => {
                if self.enable_ram_and_rtc {
                    match self.ram_bank_or_rtc_select {
                        RamBankOrRtcSelect::Ram { idx } => {
                            self.ram_banks[idx as usize].as_mut_slice()[addr as usize - 0xA000] =
                                byte;
                        }
                        // TODO: implement writes to clock register
                        RamBankOrRtcSelect::Seconds => {
                            self.clock_registers.seconds = byte % 60;
                        }
                        RamBankOrRtcSelect::Minutes => {
                            self.clock_registers.minutes = byte % 60;
                        }
                        RamBankOrRtcSelect::Hours => {
                            self.clock_registers.hours = byte % 24;
                        }
                        RamBankOrRtcSelect::DayCounterLoBits => {
                            self.clock_registers.days_low = byte;
                        }
                        RamBankOrRtcSelect::DayCounterHiBits => {
                            self.clock_registers.days_hi_bit = (byte & 0x01) != 0;
                            self.clock_registers.day_counter_carry = (byte & 0x80) != 0;
                        }
                    }
                }
            }
            _ => panic!("Illegal write to cartridge: {} <- {}", addr, byte),
        }
    }

    fn set_rom(&mut self, rom: &[u8]) {
        let banks = parse_banks(rom);
        self.rom_banks = banks;
    }
}

#[derive(Serialize, Deserialize, Clone)]
pub struct RomBank(#[serde(with = "BigArray")] pub [u8; 0x4000]);

#[derive(Serialize, Deserialize, Clone)]
pub struct RamBank(#[serde(with = "BigArray")] pub [u8; 0x2000]);
impl RomBank {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }
}

impl RamBank {
    fn as_slice(&self) -> &[u8] {
        &self.0
    }

    fn as_mut_slice(&mut self) -> &mut [u8] {
        &mut self.0
    }
}
