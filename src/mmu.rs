use enumset::{EnumSet, EnumSetType};
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::ppu::{
    self, BgAndWindowTileDataArea, ColorPalette, LcdStatus, ObjColorPaletteIdx, ObjSize, Ppu,
    Priority, TileMapArea,
};
use crate::timer::{Timer, TimerFrequency};
use crate::util::U8Ext;
use crate::{cartridge, joypad};
use cartridge::Cartridge;
use core::panic;
use joypad::Button;

pub trait Memory {
    fn read_byte(&self, addr: u16) -> u8;
    fn write_byte(&mut self, addr: u16, byte: u8);
    fn step(&mut self, t_cycles: u8);
    fn interrupts_enabled(&self) -> EnumSet<InterruptKind>;
    fn interrupts_requested(&self) -> EnumSet<InterruptKind>;
    fn clear_requested_interrupt(&mut self, interrupt: InterruptKind);

    fn pressed_buttons(&self) -> EnumSet<Button>;
    fn set_pressed_buttons(&mut self, buttons: EnumSet<Button>);
    fn in_boot_rom(&self) -> bool;
    fn set_not_in_boot_rom(&mut self);

    fn ppu_as_ref(&self) -> &Ppu;

    fn read_word(&self, addr: u16) -> u16 {
        let lo = self.read_byte(addr);
        let hi = self.read_byte(addr + 1);
        u16::from_le_bytes([lo, hi])
    }

    fn write_word(&mut self, addr: u16, word: u16) {
        let [lo, hi] = word.to_le_bytes();
        self.write_byte(addr, lo);
        self.write_byte(addr + 1, hi);
    }

    fn set_cart_rom(&mut self, rom: &[u8]);
}

#[derive(Serialize, Deserialize)]
pub struct Mmu {
    cartridge: Box<dyn Cartridge>,
    #[serde(with = "BigArray")]
    work_ram: [u8; 0x2000],
    #[serde(with = "BigArray")]
    high_ram: [u8; 0x80],
    #[serde(with = "BigArray")]
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
    pub timer: Timer,
    /// TODO: reset when executing STOP instruction and only begin ticking once stop mode ends
    pub divider: Timer,
    joypad_select: JoypadSelect,
    pub pressed_buttons: EnumSet<joypad::Button>,
}

impl Mmu {
    pub fn new(rom: &[u8]) -> Self {
        let mbc_type = rom[0x0147];
        let cartridge: Box<dyn Cartridge> = match mbc_type {
            0x00 | 0x08 | 0x09 => Box::new(cartridge::NoMbc::from_game_rom(rom)),
            0x01..=0x03 => {
                // MBC1
                Box::new(cartridge::Mbc1::from_game_rom(rom))
            }
            0x0F..=0x13 => {
                // MBC3
                Box::new(cartridge::Mbc3::from_game_rom(rom))
            }
            0x19..=0x1E => {
                todo!("Support MBC 5")
            }
            _ => {
                todo!("Unsupported MBC: {:0X}", mbc_type)
            }
        };
        Mmu {
            cartridge,
            work_ram: [0; 0x2000],
            high_ram: [0; 0x80],
            ppu: Ppu::new(),
            interrupts_enabled: EnumSet::empty(),
            interrupts_requested: EnumSet::empty(),
            timer: Timer::disabled(TimerFrequency::F4KiHz),
            divider: Timer::enabled(TimerFrequency::F16KiHz),
            boot_rom: *include_bytes!("../roms/dmg_boot.bin"),
            in_boot_rom: true,
            joypad_select: JoypadSelect::None,
            pressed_buttons: EnumSet::empty(),
        }
    }
}

impl Memory for Mmu {
    fn read_byte(&self, addr: u16) -> u8 {
        match addr {
            // ROM
            0x0000..=0x7FFF => {
                if self.in_boot_rom && addr < 0x100 {
                    self.boot_rom[addr as usize]
                } else {
                    self.cartridge.read(addr)
                }
            }
            // VRAM
            0x8000..=0x9FFF => self.ppu.read_vram_byte(addr),
            // external RAM
            0xA000..=0xBFFF => self.cartridge.read(addr),
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
                let (select_hi, select_lo) = self.joypad_select.to_be_bits();
                // If a button is pressed, the corresponding bit is 0, not 1!
                let btn_state = |button: Button| !self.pressed_buttons.contains(button);
                use Button::*;
                match self.joypad_select {
                    JoypadSelect::Buttons => u8::from_bits([
                        true,
                        true,
                        select_hi,
                        select_lo,
                        btn_state(Start),
                        btn_state(Select),
                        btn_state(B),
                        btn_state(A),
                    ]),
                    JoypadSelect::DPad => u8::from_bits([
                        true,
                        true,
                        select_hi,
                        select_lo,
                        btn_state(Down),
                        btn_state(Up),
                        btn_state(Left),
                        btn_state(Right),
                    ]),
                    // If neither buttons nor d-pad is selected ($30 was written), then the low nibble reads $F (all buttons released).
                    JoypadSelect::None => {
                        u8::from_bits([true, true, true, true, true, true, true, true])
                    }
                    JoypadSelect::All => u8::from_bits([
                        true,
                        true,
                        select_hi,
                        select_lo,
                        btn_state(Start) || btn_state(Down),
                        btn_state(Select) || btn_state(Up),
                        btn_state(B) || btn_state(Left),
                        btn_state(A) || btn_state(Right),
                    ]),
                }
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
                0x00
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
            0xFF42 => self.ppu.viewport_offset.y,
            0xFF43 => self.ppu.viewport_offset.x,
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

    fn write_byte(&mut self, addr: u16, byte: u8) {
        // println!("MMU: Write byte {:#X}: {:#X}", addr, byte);
        match addr {
            // ROM banks
            0x0000..=0x7FFF => {
                self.cartridge.write(addr, byte);
            }
            // VRAM
            0x8000..=0x9FFF => {
                self.ppu.write_vram_byte(addr, byte);
            }
            // external RAM
            0xA000..=0xBFFF => self.cartridge.write(addr, byte),
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
                        obj.bg_over_obj_priority = match priority {
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
                if !lcd_enable {
                    // turn ppu off
                    self.ppu.line = 0;
                    self.ppu.mode = ppu::Mode::HorizontalBlank;
                    self.ppu.cycles_in_mode = 0
                }
                self.ppu.lcd_enabled = lcd_enable;
                self.ppu.bg_tile_map_select = TileMapArea::from_bit(bg_tile_map_area_bit);
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
                // if self.ppu.viewport_offset.y != byte {
                // let now = std::time::Instant::now();
                // let duration = now - self.ppu.last_viewport_update;
                // println!(
                //     "Viewport y changed from {:?} to {:?} after {:?}, during LCD mode {:?}",
                //     self.ppu.viewport_offset.y, byte, duration, self.ppu.mode
                // );
                // self.ppu.last_viewport_update = now;
                // }
                self.ppu.viewport_offset.y = byte;
            }
            0xFF43 => {
                // if self.ppu.viewport_offset.x != byte {
                //     let now = std::time::Instant::now();
                //     let duration = now - self.ppu.last_viewport_update;
                //     println!(
                //         "Viewport x changed from {:?} to {:?} after {:?} during LCD mode {:?}",
                //         self.ppu.viewport_offset.x, byte, duration, self.ppu.mode
                //     );
                //     self.ppu.last_viewport_update = now;
                // }
                self.ppu.viewport_offset.x = byte;
            }
            0xFF44 => {
                eprintln!("ROM attempted to write to 0xFF44 which is a read-only IO register for the current LCD Y-position");
            }
            0xFF45 => {
                self.ppu.lyc = byte;
            }
            0xFF46 => {
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
            0xFF68..=0xFF69 => {
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

    fn step(&mut self, t_cycles: u8) {
        let overflowed = self.timer.update(t_cycles);
        if overflowed {
            self.interrupts_requested |= InterruptKind::Timer;
        }
        let ppu_interrupts = self.ppu.step(t_cycles);
        self.interrupts_requested |= ppu_interrupts;

        self.divider.update(t_cycles);
    }

    fn interrupts_enabled(&self) -> EnumSet<InterruptKind> {
        self.interrupts_enabled
    }

    fn interrupts_requested(&self) -> EnumSet<InterruptKind> {
        self.interrupts_requested
    }

    fn pressed_buttons(&self) -> EnumSet<Button> {
        self.pressed_buttons
    }

    fn set_pressed_buttons(&mut self, buttons: EnumSet<Button>) {
        self.pressed_buttons = buttons;
    }

    fn in_boot_rom(&self) -> bool {
        self.in_boot_rom
    }

    fn set_not_in_boot_rom(&mut self) {
        self.in_boot_rom = false;
    }

    fn ppu_as_ref(&self) -> &Ppu {
        &self.ppu
    }

    fn clear_requested_interrupt(&mut self, interrupt: InterruptKind) {
        self.interrupts_requested.remove(interrupt);
    }
    fn set_cart_rom(&mut self, rom: &[u8]) {
        self.cartridge.set_rom(rom);
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
#[derive(Serialize, Deserialize)]
enum JoypadSelect {
    All,
    Buttons,
    DPad,
    None,
}

impl JoypadSelect {
    fn from_be_bits(hi: bool, lo: bool) -> Self {
        match (hi, lo) {
            (false, false) => JoypadSelect::All,
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
            JoypadSelect::All => (false, false),
        }
    }
}

#[cfg(test)]
mod tests {
    use ppu::ObjectAttributes;

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
        let mut mmu = Mmu::new(&[0; 0x8000]);
        for addr in 0xFE00..=0xFE9F {
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
                bg_over_obj_priority: Priority::One,
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
