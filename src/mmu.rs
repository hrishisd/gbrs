#![allow(unused)]

use crate::ppu::{
    self, BgAndWindowTileDataArea, ColorPalette, LcdStatus, ObjSize, Ppu, TileMapArea,
};
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
    ppu: Ppu,
    /// A set of flags that indicates whether the interrupt handler for each corresponding piece of hardware may be called.
    ///
    /// also referred to as `IE`
    pub interrupts_enabled: InterruptFlags,
    /// A set of flags indicates that an interrupt has been signaled.
    ///
    /// Any set flags only indicate that an interrupt is being *requested*. The actual *execution* of the interrupt handler only happens if both the `IME` register and the corresponding flag in `IE` are set.
    pub interrupts_requested: InterruptFlags,
    /// TODO: handle timer interrupts
    timer: Timer,
    /// TODO: reset when executing STOP instruction and only begin ticking once stop mode ends
    divider: Timer,
}

impl Mmu {
    pub fn create(rom: &[u8]) -> Self {
        let mut rom_bank_0 = [0; 0x4000];
        let upto_idx = rom_bank_0.len().min(rom.len());
        rom_bank_0[..upto_idx].copy_from_slice(&rom[..upto_idx]);

        // TODO: initialize other rom banks
        let mut rom_bank_n = [0; 0x4000];
        if rom.len() > 0x4000 {
            let n_bytes = rom_bank_n.len().min(rom.len() - 0x4000);
            rom_bank_n[..n_bytes].copy_from_slice(&rom[0x4000..(0x4000 + n_bytes)]);
        }
        Mmu {
            rom_bank_0,
            rom_bank_n,
            ext_ram: [0; 0x2000],
            work_ram: [0; 0x2000],
            high_ram: [0; 0x80],
            ppu: Ppu::new(),
            interrupts_enabled: InterruptFlags::from(0x00),
            interrupts_requested: InterruptFlags::from(0x00),
            timer: Timer::new(TimerFrequency::F4KiHz),
            divider: Timer::new(TimerFrequency::F16KiHz),
        }
    }

    pub fn step(&mut self, t_cycles: u8) {
        let overflowed = self.timer.update(t_cycles);
        if overflowed {
            self.interrupts_requested.timer = true;
        }
        self.ppu.step(t_cycles);
        self.divider.update(t_cycles);
        // TODO: return requested interrupts to CPU?
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
                self.ppu.vram[addr as usize - 0x8000]
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
            0xFF00 => {
                // joypad input
                todo!("implement joypad input")
            }
            0xFF01 | 0xFF02 => 0, // TODO: serial
            0xFF04 => self.divider.value,
            0xFF05 => self.timer.value,
            0xFF06 => self.timer.tma,
            0xFF07 => {
                let [freq_hi, freq_lo] = {
                    match self.timer.frequency {
                        TimerFrequency::F4KiHz => [false, false],
                        TimerFrequency::F16KiHz => [true, true],
                        TimerFrequency::F64KiHz => [true, false],
                        TimerFrequency::F256KiHz => [false, true],
                    }
                };
                u8::from_bits([
                    true,
                    true,
                    true,
                    true,
                    true,
                    self.timer.enabled,
                    freq_hi,
                    freq_lo,
                ])
            }
            0xFF0F => self.interrupts_requested.into(),
            0xFF10..=0xFF3F => {
                // TODO: audio
                0
            }
            // LCD control
            0xFF40 => u8::from_bits([
                self.ppu.lcd_enabled,
                self.ppu.window_tile_map_area.to_bit(),
                self.ppu.window_enabled,
                self.ppu.bg_and_window_data_tile_area.to_bit(),
                self.ppu.bg_tile_map_area.to_bit(),
                self.ppu.obj_size.to_bit(),
                self.ppu.obj_enabled,
                self.ppu.bg_enabled,
            ]),
            // LCD status
            0xFF41 => {
                use ppu::Mode;
                let (b1, b0) = match self.ppu.mode {
                    Mode::HorizontalBlank => (false, false),
                    Mode::VerticalBlank => (false, true),
                    Mode::ScanlineOAM => (true, false),
                    Mode::ScanlineVRAM => (true, true),
                };
                let stat = self.ppu.lcd_status;
                u8::from_bits([
                    true,
                    stat.lyc_int_select,
                    stat.mode_2_int_select,
                    stat.mode_1_int_select,
                    stat.mode_0_int_select,
                    stat.lyc_eq_lq,
                    b1,
                    b0,
                ])
            }
            // Background viewport position
            0xFF42 => self.ppu.bg_viewport_offset.y,
            0xFF43 => self.ppu.bg_viewport_offset.x,
            0xFF44 => self.ppu.line,
            0xFF45 => self.ppu.lyc,
            0xFF46 => {
                todo!("OAM DMA source address and start")
            }
            0xFF47 => self.ppu.bg_color_palette.into(),
            0xFF48 => self.ppu.obj_color_palettes[0].into(),
            0xFF49 => self.ppu.obj_color_palettes[1].into(),
            // Window position
            0xFF4A => {
                todo!("SCY background viewport y position")
            }
            0xFF4B => {
                todo!("SCX background viewport x position")
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
            0xFF68..=0xFF6B => {
                // BG / OBJ palettes
                0
            }
            0xFF70 => {
                // WRAM bank select
                0
            }
            // high ram?
            0xFF80..=0xFFFE => self.high_ram[addr as usize - 0xFF80],
            // interrupt enable register
            0xFFFF => self.interrupts_enabled.into(),
            _ => panic!("Unhandled IO register read for addr: {addr:X}"),
        }
    }

    pub fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr);
        let hi = self.read_byte(addr + 1);
        // println!(
        //     "MMU: Read word {:#X}: {:#X}",
        //     addr,
        //     u16::from_le_bytes([lo, hi])
        // );
        u16::from_le_bytes([lo, hi])
    }

    pub fn write_byte(&mut self, addr: u16, byte: u8) {
        // println!("MMU: Write byte {:#X}: {:#X}", addr, byte);
        match addr {
            // ROM bank 0
            0x0000..=0x3FFF => self.rom_bank_0[addr as usize] = byte,
            // ROM bank 01-NN
            0x4000..=0x7FFF => self.rom_bank_n[(addr & 0x3FFF) as usize] = byte,
            // VRAM
            0x8000..=0x9FFF => {
                self.ppu.vram[addr as usize - 0x8000] = byte;
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
                    // This is a noop to pass Blargg's test ROMs
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
                    self.interrupts_requested = InterruptFlags::from(byte);
                }
                0xFF10..=0xFF26 => {
                    // TODO: implement audio
                }
                0xFF30..=0xFF3F => {
                    // wave pattern
                    todo!();
                }
                // LCD control
                0xFF40 => {
                    let [lcd_enable, window_tile_map_bit, window_enable, bg_and_window_tile_data_bit, bg_tile_map_area_bit, obj_size_bit, obj_enable, bg_enable] =
                        byte.bits();
                    // TODO: assert that lcd only goes from false->true when ppu is in VBlank mode
                    self.ppu.lcd_enabled = lcd_enable;
                    self.ppu.window_tile_map_area = TileMapArea::from_bit(window_tile_map_bit);
                    self.ppu.window_enabled = window_enable;
                    self.ppu.bg_and_window_data_tile_area = if bg_and_window_tile_data_bit {
                        BgAndWindowTileDataArea::X8000
                    } else {
                        BgAndWindowTileDataArea::X8800
                    };
                    self.ppu.obj_size = if obj_size_bit {
                        ObjSize::Dim8x16
                    } else {
                        ObjSize::Dim8x8
                    };
                    self.ppu.obj_enabled = obj_enable;
                    self.ppu.bg_enabled = bg_enable;
                }
                // LCD status
                0xFF41 => {
                    let [_, lyc_int_select, mode_2_int_select, mode_1_int_select, mode_0_int_select, _, _, _] =
                        byte.bits();
                    self.ppu.lcd_status = LcdStatus {
                        lyc_int_select,
                        mode_2_int_select,
                        mode_1_int_select,
                        mode_0_int_select,
                        ..self.ppu.lcd_status
                    }
                }
                // Background viewport position
                0xFF42 => {
                    self.ppu.bg_viewport_offset.y = byte;
                }
                0xFF43 => {
                    self.ppu.bg_viewport_offset.x = byte;
                }
                0xFF44 => {
                    panic!("ROM attempted to write to 0xFF44 which is a read-only IO register for the current LCD Y-position");
                }
                0xFF45 => {
                    self.ppu.lyc = byte;
                }
                0xFF47 => self.ppu.bg_color_palette = ColorPalette::from(byte),
                0xFF48 => self.ppu.obj_color_palettes[0] = ColorPalette::from(byte),
                0xFF49 => self.ppu.obj_color_palettes[1] = ColorPalette::from(byte),
                // Window position
                0xFF4A => {
                    todo!("SCY background viewport y position")
                }
                0xFF4B => {
                    todo!("SCX background viewport x position")
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
            0xFFFF => self.interrupts_enabled = InterruptFlags::from(byte),
        }
    }

    pub fn write_word(&mut self, addr: u16, word: u16) {
        // println!("MMU: Write word {:#X}: {:#X}", addr, word);
        let [lo, hi] = word.to_le_bytes();
        self.write_byte(addr, lo);
        self.write_byte(addr + 1, hi);
    }
}

/// Flags for each interrupt handler
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InterruptFlags {
    joypad: bool,
    serial: bool,
    timer: bool,
    lcd: bool,
    vblank: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InterruptKind {
    Joypad,
    Serial,
    Timer,
    Lcd,
    Vblank,
}

impl InterruptFlags {
    pub fn get(self, kind: InterruptKind) -> bool {
        match kind {
            InterruptKind::Joypad => self.joypad,
            InterruptKind::Serial => self.serial,
            InterruptKind::Timer => self.timer,
            InterruptKind::Lcd => self.lcd,
            InterruptKind::Vblank => self.vblank,
        }
    }

    pub fn set(self, interrupt_kind: InterruptKind, arg: bool) -> Self {
        match interrupt_kind {
            InterruptKind::Joypad => Self {
                joypad: arg,
                ..self
            },
            InterruptKind::Serial => Self {
                serial: arg,
                ..self
            },
            InterruptKind::Timer => Self { timer: arg, ..self },
            InterruptKind::Lcd => Self { lcd: arg, ..self },
            InterruptKind::Vblank => Self {
                vblank: arg,
                ..self
            },
        }
    }
}

impl From<u8> for InterruptFlags {
    fn from(byte: u8) -> Self {
        let [_, _, _, joypad, serial, timer, lcd, vblank] = byte.bits();
        InterruptFlags {
            joypad,
            serial,
            timer,
            lcd,
            vblank,
        }
    }
}

impl From<InterruptFlags> for u8 {
    fn from(val: InterruptFlags) -> Self {
        u8::from_bits([
            false, false, false, val.joypad, val.serial, val.timer, val.lcd, val.vblank,
        ])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn from_byte() {
        let flags = InterruptFlags::from(0b00011111);
        assert!(flags.joypad);
        assert!(flags.serial);
        assert!(flags.timer);
        assert!(flags.lcd);
        assert!(flags.vblank);

        let byte = 0b00011000;
        let flags = InterruptFlags::from(byte);
        assert!(flags.joypad);
        assert!(flags.serial);
        assert_eq!(flags.timer, false);
        assert_eq!(flags.lcd, false);
        assert_eq!(flags.vblank, false);

        let flags_as_byte: u8 = flags.into();
        assert_eq!(flags_as_byte, byte);
    }
}
