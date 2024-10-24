#![allow(unused)]

use enumset::{EnumSet, EnumSetType};
use proptest::sample::select;
use sdl2::sys::SelectionNotify;

use crate::ppu::{
    self, BgAndWindowTileDataArea, ColorPalette, LcdStatus, ObjColorPaletteIdx, ObjSize,
    ObjectAttributes, Ppu, Priority, TileMapArea,
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
    boot_rom: [u8; 0x100],
    pub in_boot_rom: bool,
    pub ppu: Ppu,
    /// A set of flags that indicates whether the interrupt handler for each corresponding piece of hardware may be called.
    ///
    /// also referred to as `IE`
    pub interrupts_enabled: EnumSet<InterruptKind>,
    /// A set of flags indicates that an interrupt has been signaled.
    ///
    /// Any set flags only indicate that an interrupt is being *requested*. The actual *execution* of the interrupt handler only happens if both the `IME` register and the corresponding flag in `IE` are set.
    pub interrupts_requested: EnumSet<InterruptKind>,
    timer: Timer,
    /// TODO: reset when executing STOP instruction and only begin ticking once stop mode ends
    divider: Timer,
    joypad_select: JoypadSelect,
}

impl Mmu {
    pub fn create(rom: &[u8]) -> Self {
        let boot_rom = include_bytes!("../roms/dmg_boot.bin");
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
            interrupts_enabled: EnumSet::empty(),
            interrupts_requested: EnumSet::empty(),
            timer: Timer::disabled(TimerFrequency::F4KiHz),
            divider: Timer::disabled(TimerFrequency::F16KiHz),
            boot_rom: *boot_rom,
            in_boot_rom: true,
            joypad_select: JoypadSelect::None,
        }
    }

    pub fn _debug_mode(rom: &[u8]) -> Self {
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
            interrupts_enabled: EnumSet::empty(),
            interrupts_requested: EnumSet::empty(),
            timer: Timer::disabled(TimerFrequency::F4KiHz),
            divider: Timer::enabled(TimerFrequency::F16KiHz),
            boot_rom: [0; 256],
            in_boot_rom: false,
            joypad_select: JoypadSelect::None,
        }
    }

    pub fn step(&mut self, t_cycles: u8) {
        let overflowed = self.timer.update(t_cycles);
        // println!("Timer: {:#?}, overflowed: {:#?}", self.timer, overflowed);
        if overflowed {
            self.interrupts_requested |= InterruptKind::Timer;
        }
        let ppu_interrupts = self.ppu.step(t_cycles);
        self.interrupts_requested |= ppu_interrupts;

        self.divider.update(t_cycles);
    }

    pub fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM bank 0
            0x0000..=0x3FFF => {
                if self.in_boot_rom && addr < 0x100 {
                    self.boot_rom[addr as usize]
                } else {
                    self.rom_bank_0[addr as usize]
                }
            }
            // ROM bank 01-NN
            0x4000..=0x7FFF => self.rom_bank_n[(addr & 0x3FFF) as usize],
            // VRAM
            0x8000..=0x9FFF => self.ppu.read_vram_byte(addr),
            // external RAM
            0xA000..=0xBFFF => self.ext_ram[(addr & 0x1FFF) as usize],
            // work RAM
            0xC000..=0xDFFF => self.work_ram[(addr & 0x1FFF) as usize],
            // echo RAM
            0xE000..=0xFDFF => self.work_ram[(addr & 0x1FFF) as usize],
            // object attribute memory
            0xFE00..=0xFE9F => {
                // The obj entry is 4 bytes
                let object_entry_idx = (addr - 0xFE00) >> 2;
                assert!(
                    (0..40).contains(&object_entry_idx),
                    "invalid obj entry idx: {object_entry_idx} calculated from address {addr}"
                );
                let object_attributes = self.ppu.obj_attribute_memory[object_entry_idx as usize];
                let byte_offset = addr % 4;
                object_attributes.as_bytes()[byte_offset as usize]
            }
            // not usable
            0xFEA0..=0xFEFF => {
                panic!("Program accessed invalid memory: {addr:X}")
            }
            // io registers
            0xFF00 => {
                // If a button is pressed, the corresponding bit is 0, not 1!
                let (select_hi, select_lo) = self.joypad_select.to_be_bits();
                // If neither buttons nor d-pad is selected ($30 was written), then the low nibble reads $F (all buttons released).
                // TODO: handle actual joypad input
                u8::from_bits([true, true, select_hi, select_lo, true, true, true, true])
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
            0xFF0F => self.interrupts_requested.as_u8(),
            0xFF10..=0xFF3F => {
                // TODO: audio
                0
            }
            // LCD control
            0xFF40 => u8::from_bits([
                self.ppu.lcd_enabled,
                self.ppu.window_tile_map_select.to_bit(),
                self.ppu.window_enabled,
                self.ppu.bg_and_window_tile_data_select.to_bit(),
                self.ppu.bg_tile_map_select.to_bit(),
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
                    self.ppu.line == self.ppu.lyc,
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
                panic!("Attempted to read from DMA transfer register")
            }
            0xFF47 => self.ppu.bg_color_palette.into(),
            0xFF48 => self.ppu.obj_color_palettes[0].into(),
            0xFF49 => self.ppu.obj_color_palettes[1].into(),
            // Window position
            0xFF4A => self.ppu.window_top_left.y,
            0xFF4B => self.ppu.window_top_left.x,
            0xFF4D => {
                // todo!("CGB mode only, prepare speed switch")
                0xFF
            }
            0xFF4F => {
                // todo!("CGB mode only, VRAM bank select")
                0xFF
            }
            0xFF50 => {
                // set to non-zero to disable boot ROM
                panic!("Attempted to read from boot ROM disable register")
            }
            0xFF51..=0xFF55 => {
                // VRAM DMA
                // todo!("CGB mode only, LCD VRAM DMA transfers")
                0xFF
            }
            0xFF68..=0xFF6B => {
                // todo!("CGB only, BG/OBJ Palettes")
                0xFF
            }
            0xFF70 => {
                // todo!("CGB mode only, WRAM Bank select")
                0xFF
            }
            // high ram?
            0xFF80..=0xFFFE => self.high_ram[addr as usize - 0xFF80],
            // interrupt enable register
            0xFFFF => self.interrupts_enabled.as_u8(),
            _ => panic!("Unhandled register read for addr: {addr:X}"),
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
                self.ppu.write_vram_byte(addr, byte);
            }
            // external RAM
            0xA000..=0xBFFF => self.ext_ram[(addr & 0x1FFF) as usize] = byte,
            // work RAM
            0xC000..=0xDFFF => self.work_ram[(addr & 0x1FFF) as usize] = byte,
            // echo RAM
            0xE000..=0xFDFF => self.work_ram[(addr & 0x1FFF) as usize] = byte,
            // object attribute memory
            0xFE00..=0xFE9F => {
                // The obj entry is 4 bytes
                let object_entry_idx = (addr - 0xFE00) >> 2;
                assert!(
                    (0..40).contains(&object_entry_idx),
                    "invalid obj entry idx: {object_entry_idx} calculated from address {addr}"
                );
                let obj = &mut self.ppu.obj_attribute_memory[object_entry_idx as usize];
                let byte_offset = addr % 4;
                match byte_offset {
                    0 => obj.y_pos = byte,
                    1 => obj.x_pos = byte,
                    2 => obj.tile_idx = byte,
                    3 => {
                        // WARNING: This strategy throws away the parts of the byte that are used in CGB mode
                        let [priority, y_flip, x_flip, dmg_palette, _, _, _, _] = byte.bits();
                        obj.y_flip = y_flip;
                        obj.x_flip = x_flip;
                        obj.priority = match priority {
                            true => Priority::One,
                            false => Priority::Zero,
                        };
                        obj.palette = match dmg_palette {
                            true => ObjColorPaletteIdx::One,
                            false => ObjColorPaletteIdx::Zero,
                        };
                    }
                    _ => panic!("BUG"),
                }
            }
            // not usable
            0xFEA0..=0xFEFF => {}
            // io registers
            0xFF00 => {
                // joypad input
                let [_, _, select_hi, select_lo, _, _, _, _] = byte.bits();
                let joypad_select = JoypadSelect::from_be_bits(select_hi, select_lo);
                self.joypad_select = joypad_select;
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
                let [.., enable, clock_select_1, clock_select_0] = byte.bits();
                let frequency = match [clock_select_1, clock_select_0] {
                    [false, false] => TimerFrequency::F4KiHz,
                    [false, true] => TimerFrequency::F256KiHz,
                    [true, false] => TimerFrequency::F64KiHz,
                    [true, true] => TimerFrequency::F16KiHz,
                };
                self.timer.enabled = enable;
                self.timer.frequency = frequency;
            }
            0xFF0F => self.interrupts_requested = EnumSet::<InterruptKind>::from_u8_truncated(byte),
            0xFF10..=0xFF26 => {
                // TODO: implement audio
            }
            0xFF30..=0xFF3F => {
                // wave pattern
                // TODO implement audio
            }
            // LCD control
            0xFF40 => {
                let [lcd_enable, window_tile_map_bit, window_enable, bg_and_window_tile_data_bit, bg_tile_map_area_bit, obj_size_bit, obj_enable, bg_enable] =
                    byte.bits();
                // TODO: assert that lcd only goes from false->true when ppu is in VBlank mode
                self.ppu.lcd_enabled = lcd_enable;
                self.ppu.window_tile_map_select = TileMapArea::from_bit(window_tile_map_bit);
                self.ppu.window_enabled = window_enable;
                self.ppu.bg_and_window_tile_data_select = if bg_and_window_tile_data_bit {
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
                eprintln!("ROM attempted to write to 0xFF44 which is a read-only IO register for the current LCD Y-position");
            }
            0xFF45 => {
                self.ppu.lyc = byte;
            }
            0xFF46 => {
                // TODO: This would be more performant if we write directly to the oam attributes array in the PPU
                // Perform OAM DMA transfer.
                // DMA on the real system takes 160 µs to complete.
                // This implementation doesn't simulate the DMA timing.
                let source_addr = (byte as u16) << 8;
                let dest_addr = 0xFE00;
                for offset in 0..0xA0 {
                    self.write_byte(dest_addr + offset, self.read_byte(source_addr + offset));
                }
            }
            0xFF47 => self.ppu.bg_color_palette = ColorPalette::from(byte),
            0xFF48 => self.ppu.obj_color_palettes[0] = ColorPalette::from(byte),
            0xFF49 => self.ppu.obj_color_palettes[1] = ColorPalette::from(byte),
            // Window position
            0xFF4A => self.ppu.window_top_left.y = byte,
            0xFF4B => self.ppu.window_top_left.x = byte,
            0xFF4D => {
                // todo!("CGB mode only, prepare speed switch")
            }
            0xFF4F => {
                // todo!("CGB mode only, VRAM bank select")
            }
            0xFF50 => {
                // set to non-zero to disable boot ROM
                if byte != 0 {
                    self.in_boot_rom = false;
                }
            }
            0xFF51..=0xFF55 => {
                // TODO VRAM DMA (CDB mode only)
            }
            0xFF68..=0xFF6B => {
                // TODO: BG / OBJ palettes (CGB mode only)
            }
            0xFF6A => {
                // Obj color palette spec (CGB mode only)
            }
            0xFF6B => {
                // Obj color palette data (CGB mode only)
            }
            0xFF6C => {
                // Obj priority mode (CGB mode only)
            }
            0xFF70 => {
                // WRAM bank select (CGB only)
            }
            // high ram, used by LDH instructions
            0xFF80..=0xFFFE => {
                self.high_ram[addr as usize - 0xFF80] = byte;
            }
            // interrupt enable register
            0xFFFF => self.interrupts_enabled = EnumSet::<InterruptKind>::from_u8_truncated(byte),
            _ => eprintln!("unhandled register write for addr: {addr:X}"),
        }
    }

    pub fn write_word(&mut self, addr: u16, word: u16) {
        // println!("MMU: Write word {:#X}: {:#X}", addr, word);
        let [lo, hi] = word.to_le_bytes();
        self.write_byte(addr, lo);
        self.write_byte(addr + 1, hi);
    }
}

/// This type's u8 representation directly corresponds to the interrupt flags' u8 representation in memory.
#[derive(Debug, EnumSetType)]
#[enumset(repr = "u8")]
pub enum InterruptKind {
    Vblank = 0,
    LcdStat = 1,
    Timer = 2,
    Serial = 3,
    Joypad = 4,
}

/// Configures whether the joypad register returns the state of the buttons or the direction keys.
enum JoypadSelect {
    Buttons,
    DPad,
    None,
}

impl JoypadSelect {
    fn from_be_bits(hi: bool, lo: bool) -> Self {
        match (hi, lo) {
            (false, false) => panic!("Can't select buttons and dpad at the same time"),
            (false, true) => JoypadSelect::Buttons,
            (true, false) => JoypadSelect::DPad,
            (true, true) => JoypadSelect::None,
        }
    }
    fn to_be_bits(&self) -> (bool, bool) {
        match self {
            JoypadSelect::Buttons => (false, true),
            JoypadSelect::DPad => (true, false),
            JoypadSelect::None => (true, true),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn interrupts_from_byte() {
        let flags = EnumSet::<InterruptKind>::from_u8(0b00011111);
        let all_set = EnumSet::all();
        assert_eq!(flags, all_set);

        let byte = 0b00011000;
        let flags = EnumSet::<InterruptKind>::from_u8(byte);
        assert_eq!(flags, InterruptKind::Joypad | InterruptKind::Serial);

        let flags = EnumSet::<InterruptKind>::from_u8(0);
        assert_eq!(flags, EnumSet::empty());

        let flags = EnumSet::<InterruptKind>::from_u8_truncated(0xFF);
        let all_set = EnumSet::all();
        assert_eq!(flags, all_set);
    }

    #[test]
    fn oam_memory_rw() {
        let empty_arr = [];
        let mut mmu = Mmu::create(&empty_arr);
        for addr in 0xFE00..=0xFe9F {
            assert_eq!(mmu.read_byte(addr), 0);
        }
        let obj_addr = 0xFE04;
        let y_pos = 5;
        let x_pos = 10;
        let tile_idx = 20;
        let attributes = 0b1010_0000;
        mmu.write_byte(obj_addr, y_pos);
        mmu.write_byte(obj_addr + 1, x_pos);
        mmu.write_byte(obj_addr + 2, tile_idx);
        mmu.write_byte(obj_addr + 3, attributes);

        assert_eq!(
            mmu.ppu.obj_attribute_memory[1],
            ObjectAttributes {
                y_pos,
                x_pos,
                tile_idx,
                priority: Priority::One,
                y_flip: false,
                x_flip: true,
                palette: ObjColorPaletteIdx::Zero
            }
        );

        assert_eq!(mmu.read_byte(obj_addr), y_pos);
        assert_eq!(mmu.read_byte(obj_addr + 1), x_pos);
        assert_eq!(mmu.read_byte(obj_addr + 2), tile_idx);
        assert_eq!(mmu.read_byte(obj_addr + 3), attributes);
    }
}
