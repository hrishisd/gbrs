use core::panic;
use std::assert_matches::assert_matches;

use enumset::EnumSet;

use crate::{mmu::InterruptKind, util::U8Ext};

#[derive(Debug, Clone)]
pub struct Ppu {
    pub vram_tile_data: VRamTileData,
    /// At address 0x9800
    pub lo_tile_map: TileMap,
    /// At address 0x9C00
    pub hi_tile_map: TileMap,
    /// There are 144 visible lines (0-143) and 10 additional invisible lines (144-153)
    ///
    /// This is equivalent to the LCD y coordinate (LY)
    pub line: u8,
    /// The number of T-clock cycles spent in the current mode.
    ///
    /// Used to know when to switch modes and move the line index.
    cycles_in_mode: u32,
    pub mode: Mode,

    // -- LCD Control flags
    pub lcd_enabled: bool,
    pub window_tile_map_area: TileMapArea,
    pub window_enabled: bool,
    pub bg_and_window_tile_data_area: BgAndWindowTileDataArea,
    pub bg_tile_map_area: TileMapArea,
    /// color idx 0 is always transparent for objs.
    ///
    /// There are 2 color palettes so that the game can use all 4 available colors for objects.
    pub obj_color_palettes: [ColorPalette; 2],
    pub obj_size: ObjSize,
    pub obj_enabled: bool,
    pub bg_enabled: bool,

    /// OAM
    ///
    /// This is a sprite attribute table, 40 entries, 4 bytes each.
    pub obj_attribute_memory: [ObjectAttributes; 40],

    /// BGP
    ///
    /// Palette for background and window tiles.
    pub bg_color_palette: ColorPalette,
    /// The on-screen coordinates of the visible 160x144 pixel area within the 256x256 pixel background map.
    ///
    /// AKA SCY and SCX
    pub bg_viewport_offset: Coord,

    /// The on-screen coordinates of the window's top-left pixel (WY and WX)
    ///
    /// The window is visible, if enabled, when x is in \[0,166\] and y is in \[0, 143\]
    pub window_top_left: Coord,

    /// LCD Y compare. Used to set flags when compared with LY
    pub lyc: u8,
    /// LCD status register
    pub lcd_status: LcdStatus,
}

impl Ppu {
    pub(crate) fn new() -> Self {
        // TODO: check that enums are initialized to correct values
        Self {
            vram_tile_data: VRamTileData {
                tile_data_blocks: [[Tile {
                    lines: [TileLine {
                        color_ids: [ColorId::Id0; 8],
                    }; 8],
                }; 128]; 3],
            },
            lo_tile_map: TileMap {
                tile_indices: [[0; 32]; 32],
            },
            hi_tile_map: TileMap {
                tile_indices: [[0; 32]; 32],
            },
            line: 0,
            cycles_in_mode: 0,
            mode: Mode::ScanlineOAM,
            lcd_enabled: false,
            window_tile_map_area: TileMapArea::from_bit(false),
            window_enabled: false,
            bg_and_window_tile_data_area: BgAndWindowTileDataArea::X8800,
            bg_tile_map_area: TileMapArea::from_bit(false),
            obj_size: ObjSize::from_bit(false),
            obj_enabled: false,
            bg_enabled: false,
            bg_color_palette: ColorPalette::from(0x00),
            bg_viewport_offset: Coord { x: 0, y: 0 },
            lyc: 0,
            lcd_status: LcdStatus {
                lyc_int_select: false,
                mode_2_int_select: false,
                mode_1_int_select: false,
                mode_0_int_select: false,
            },
            obj_color_palettes: [ColorPalette::from(0x00); 2],
            window_top_left: Coord { x: 0, y: 0 },
            obj_attribute_memory: [ObjectAttributes {
                y_pos: 0,
                x_pos: 0,
                tile_idx: 0,
                priority: Priority::Zero,
                y_flip: false,
                x_flip: false,
                palette: ObjColorPaletteIdx::Zero,
            }; 40],
        }
    }

    pub(crate) fn read_vram_byte(&self, addr: u16) -> u8 {
        // Tile ID is the middle 2 bytes of the address
        match addr {
            // Tiles
            0x8000..=0x97FF => {
                let idx = TileByteIdx::from_addr(addr);
                let tile = {
                    let this = &self;
                    let block = &this.vram_tile_data.tile_data_blocks[idx.block_idx];
                    &block[idx.tile_idx]
                };
                tile.as_bytes()[idx.byte_idx]
            } // Tile map
            0x9800..=0x9FFF => {
                let tile_map = if (0x9800..=0x9BFF).contains(&addr) {
                    &self.lo_tile_map
                } else {
                    &self.hi_tile_map
                };
                let row_idx = ((addr / 32) % 32) as usize;
                let col_idx = (addr % 32) as usize;
                tile_map.tile_indices[row_idx][col_idx]
            }
            _ => {
                panic!("Invalid address into VRAM: {addr:#0x}")
            }
        }
    }

    pub(crate) fn write_vram_byte(&mut self, addr: u16, byte: u8) {
        // Tile ID is the middle 2 bytes of the address
        match addr {
            // Tiles
            0x8000..=0x97FF => {
                let idx = TileByteIdx::from_addr(addr);
                let tile = {
                    let this = &mut *self;
                    let block = &mut this.vram_tile_data.tile_data_blocks[idx.block_idx];
                    &mut block[idx.tile_idx]
                };
                let line = &mut tile.lines[idx.line_idx];
                let LineBytes { lsbs, msbs } = line.as_bytes();
                let (new_lsbs, new_msbs) = match idx.byte_idx % 2 {
                    0 => (byte, msbs),
                    1 => (lsbs, byte),
                    _ => panic!("BUG"),
                };
                *line = TileLine::from_bytes(LineBytes {
                    lsbs: new_lsbs,
                    msbs: new_msbs,
                })
            } // Tile map
            0x9800..=0x9FFF => {
                let tile_map = if (0x9800..=0x9BFF).contains(&addr) {
                    &mut self.lo_tile_map
                } else {
                    &mut self.hi_tile_map
                };
                let row_idx = ((addr / 32) % 32) as usize;
                let col_idx = (addr % 32) as usize;
                tile_map.tile_indices[row_idx][col_idx] = byte;
            }
            _ => {
                panic!("Invalid address into VRAM: {addr:#0x}")
            }
        }
    }

    pub(crate) fn step(&mut self, t_cycles: u8) -> EnumSet<InterruptKind> {
        let mut interrupts = EnumSet::empty();
        // TODO: if LCD is not enabled, do we still render?
        if !self.lcd_enabled {
            return interrupts;
        }
        self.cycles_in_mode += t_cycles as u32;
        match self.mode {
            Mode::ScanlineOAM => {
                if self.cycles_in_mode >= 80 {
                    self.cycles_in_mode -= 80;
                    self.mode = Mode::ScanlineVRAM;
                }
            }
            Mode::ScanlineVRAM => {
                if self.cycles_in_mode >= 172 {
                    self.cycles_in_mode -= 172;
                    self.mode = Mode::HorizontalBlank;
                    if self.lcd_status.mode_0_int_select {
                        interrupts |= InterruptKind::LcdStat;
                    }

                    // Now GPU has finished drawing the line, write it to the frame buffer
                    // TODO: render line here
                }
            }
            Mode::HorizontalBlank => {
                assert!(self.line < 144);
                if self.cycles_in_mode >= 204 {
                    self.cycles_in_mode -= 204;
                    self.line += 1;
                    if self.should_trigger_lyc_interrupt() {
                        interrupts |= InterruptKind::LcdStat;
                    }
                    if self.line == 144 {
                        self.mode = Mode::VerticalBlank;
                        interrupts |= InterruptKind::Vblank;
                        if self.lcd_status.mode_1_int_select {
                            interrupts |= InterruptKind::LcdStat;
                        }
                    } else {
                        assert!(self.line < 144);
                        self.mode = Mode::ScanlineOAM;
                        if self.lcd_status.mode_2_int_select {
                            interrupts |= InterruptKind::LcdStat;
                        }
                    }
                }
            }
            Mode::VerticalBlank => {
                // Once we are in this mode, line >= 144
                // Once we reach line 154, reset to line 0 and enter ScanlineOAM
                // Each line takes 456 cycles
                assert!(self.line < 154);
                if self.cycles_in_mode >= 456 {
                    self.cycles_in_mode -= 456;
                    self.line += 1;
                    if self.line == 154 {
                        self.line = 0;
                        self.mode = Mode::ScanlineOAM;
                    }
                    if self.should_trigger_lyc_interrupt() {
                        interrupts |= InterruptKind::LcdStat;
                    }
                }
            }
        }
        interrupts
    }

    /// This condition should be checked every time the current line is updated.
    fn should_trigger_lyc_interrupt(&self) -> bool {
        self.lcd_status.lyc_int_select && self.lyc == self.line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileByteIdx {
    block_idx: usize,
    tile_idx: usize,
    /// The idx of a byte in the 16 byte arrray associated with a Tile
    byte_idx: usize,
    /// The idx of a line in the 8 line arrray associated with a Tile
    line_idx: usize,
}

impl TileByteIdx {
    fn from_addr(addr: u16) -> Self {
        match addr & 0x1FFF {
            // Tiles
            0x0000..=0x17FF => {
                // There are 3 blocks of 128 tiles, where each tile has 16 bytes
                // 0b..x1 x0 y6..y0 z3..z0
                // x1x0 used to get block (0b11 is not a valid idx)
                // y6..y0 used to get tile within block
                // z3..z0 used to get idx of byte within tile
                let block_idx = ((addr & 0b1_1000_0000_0000) >> 11) as usize;
                assert_matches!(
                    block_idx,
                    0 | 1 | 2,
                    "BUG: Invalid tile block ID {block_idx}"
                );
                let tile_idx = ((addr & 0x07F0) >> 8) as usize;
                let byte_idx = (addr & 0x0F) as usize;
                // Each line consists of 2 bytes
                let line_idx = byte_idx >> 1;
                TileByteIdx {
                    block_idx,
                    tile_idx,
                    byte_idx,
                    line_idx,
                }
            }
            _ => {
                panic!("Invalid Tile address: {addr:#0x}")
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LcdStatus {
    ///  If set, selects the LYC == LY condition for the STAT interrupt
    pub lyc_int_select: bool,
    /// If set, selects the Mode 2 (OAM Scan) condition for the STAT interrupt
    pub mode_2_int_select: bool,
    /// If set, selects the Mode 1 (VBlank) condition for the STAT interrupt
    pub mode_1_int_select: bool,
    /// If set, selects the Mode 0 (HBlank) condition for the STAT interrupt
    pub mode_0_int_select: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TileMap {
    tile_indices: [[u8; 32]; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TileMapArea {
    X9800,
    X9C00,
}

impl TileMapArea {
    pub fn from_bit(b: bool) -> Self {
        if b {
            TileMapArea::X9C00
        } else {
            TileMapArea::X9800
        }
    }

    pub fn to_bit(self) -> bool {
        match self {
            TileMapArea::X9800 => false,
            TileMapArea::X9C00 => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BgAndWindowTileDataArea {
    X8800,
    X8000,
}

impl BgAndWindowTileDataArea {
    pub fn to_bit(self) -> bool {
        match self {
            BgAndWindowTileDataArea::X8800 => false,
            BgAndWindowTileDataArea::X8000 => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjSize {
    Dim8x8,
    Dim8x16,
}

impl ObjSize {
    pub fn from_bit(b: bool) -> Self {
        if b {
            ObjSize::Dim8x16
        } else {
            ObjSize::Dim8x8
        }
    }

    pub fn to_bit(self) -> bool {
        match self {
            ObjSize::Dim8x8 => false,
            ObjSize::Dim8x16 => true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Color {
    White,
    LightGray,
    DarkGray,
    Black,
}

impl Color {
    /// Create a color from bits in big-endian order.
    ///
    /// i.e. `bits[0]` is the higher-order bit
    fn from_be_bits(be_bits: [bool; 2]) -> Self {
        match be_bits {
            [false, false] => Color::White,
            [false, true] => Color::LightGray,
            [true, false] => Color::DarkGray,
            [true, true] => Color::Black,
        }
    }

    /// Convert a color into bits in big-endian order.
    ///
    /// i.e. `bits[0]` is the higher-order bit
    fn to_be_bits(self) -> [bool; 2] {
        match self {
            Color::White => [false, false],
            Color::LightGray => [false, true],
            Color::DarkGray => [true, false],
            Color::Black => [true, true],
        }
    }
}

/// field i of the strict corresponds to the ith color id
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColorPalette(Color, Color, Color, Color);

impl From<ColorPalette> for u8 {
    fn from(value: ColorPalette) -> Self {
        let id0 = value.0.to_be_bits();
        let id1 = value.1.to_be_bits();
        let id2 = value.2.to_be_bits();
        let id3 = value.3.to_be_bits();
        u8::from_bits([
            id3[1], id3[0], id2[1], id2[0], id1[1], id1[0], id0[1], id0[0],
        ])
    }
}

impl From<u8> for ColorPalette {
    fn from(value: u8) -> Self {
        let [b7, b6, b5, b4, b3, b2, b1, b0] = value.bits();
        ColorPalette(
            Color::from_be_bits([b1, b0]),
            Color::from_be_bits([b3, b2]),
            Color::from_be_bits([b5, b4]),
            Color::from_be_bits([b7, b6]),
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Mode {
    /// Takes 80 clock cycles. While in this mode, the PPU fetches assets from memory
    ScanlineOAM,
    /// Takes 172 to 289 clock cycles depending on the volume of assets being rendered
    ScanlineVRAM,
    /// Can take between 87 to 204 cycles, depending on how long mode `ScanlineVRAM` took.
    HorizontalBlank,
    /// Once the last visible row (143) has been processed, there are 10 additional rows which take 4560 clock cycles to process.
    ///
    /// After that, we go back to row 0.
    VerticalBlank,
}

#[derive(Debug, Clone)]
pub struct VRamTileData {
    tile_data_blocks: [[Tile; 128]; 3],
}

impl VRamTileData {
    /// Read a tile from blocks 0 or 1, using unsigned addressing.
    ///
    /// idx 0-127 gets from block 0
    ///
    /// idx 128-255 gets from block 1
    fn get_tile_from_0x8000(&self, idx: u8) -> Tile {
        todo!()
    }

    /// Read a tile from blocks 1 or 2 using signed addressing
    ///
    /// idx 0-127 searches within block 2
    fn get_tile_from_0x8800_signed(&self, idx: i8) -> Tile {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tile {
    /// `lines[0]` is the top-line
    lines: [TileLine; 8],
}

impl Tile {
    /// result[0] => 0th row, LSBs
    /// result[1] => 0th row, MSBs
    fn as_bytes(&self) -> [u8; 16] {
        let mut res = [0; 16];
        for (idx, line) in self.lines.iter().enumerate() {
            let bytes = line.as_bytes();
            res[2 * idx] = bytes.lsbs;
            res[2 * idx + 1] = bytes.msbs;
        }
        res
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The byte representation of the 8 colors of a line
///
/// The first byte specifies the least significant bit of the color ID of each pixel,
/// and the second byte specifies the most significant bit
///
/// In both bytes, bit 7 represents the left-most pixel and bit 0, the right-most
struct LineBytes {
    lsbs: u8,
    msbs: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileLine {
    /// The color_ids of the pixels from left to right  
    ///
    /// idx 0 represents the left-most pixel, idx 7 is the right-most pixel
    color_ids: [ColorId; 8],
}

impl TileLine {
    fn as_bytes(&self) -> LineBytes {
        use ColorId::*;
        let mut color_id_lsbs = 0;
        let mut color_id_msbs = 0;
        // bit 7 of color_id_lsbs is the lsb of the
        // *left-most* pixel
        // color_ids[0] is also the color id of the *left-most* pixel
        for (color_id_idx, color_id) in self.color_ids.iter().enumerate() {
            let bit_idx = 7 - color_id_idx as u8;
            match color_id {
                Id0 => {}
                Id1 => color_id_lsbs = color_id_lsbs.set(bit_idx),
                Id2 => color_id_msbs = color_id_msbs.set(bit_idx),
                Id3 => {
                    color_id_lsbs = color_id_lsbs.set(bit_idx);
                    color_id_msbs = color_id_msbs.set(bit_idx);
                }
            }
        }
        LineBytes {
            lsbs: color_id_lsbs,
            msbs: color_id_msbs,
        }
    }

    fn from_bytes(bytes: LineBytes) -> TileLine {
        // color_idx[0] is the left-most pixel
        // lsbs.bit(7) is the left-most pixel
        let mut color_ids = [ColorId::Id0; 8];
        for bit_idx in 0..8 {
            use ColorId::*;
            let color_id_idx = 7 - bit_idx as usize;
            color_ids[color_id_idx] = match (bytes.msbs.bit(bit_idx), bytes.lsbs.bit(bit_idx)) {
                (false, false) => Id0,
                (false, true) => Id1,
                (true, false) => Id2,
                (true, true) => Id3,
            }
        }
        TileLine { color_ids }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ColorId {
    Id0,
    Id1,
    Id2,
    Id3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ObjectAttributes {
    pub y_pos: u8,
    pub x_pos: u8,
    pub tile_idx: u8,

    // -- attributes/flags --
    pub priority: Priority,
    pub y_flip: bool,
    pub x_flip: bool,
    pub palette: ObjColorPaletteIdx,
}

impl ObjectAttributes {
    pub fn as_bytes(&self) -> [u8; 4] {
        let byte_3 = u8::from_bits([
            match self.priority {
                Priority::Zero => false,
                Priority::One => true,
            },
            self.y_flip,
            self.x_flip,
            match self.palette {
                ObjColorPaletteIdx::Zero => false,
                ObjColorPaletteIdx::One => true,
            },
            false,
            false,
            false,
            false,
        ]);
        [self.y_pos, self.x_pos, self.tile_idx, byte_3]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjColorPaletteIdx {
    Zero,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Priority {
    Zero,
    One,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tile_line_byte_conversion() {
        use ColorId::*;
        // MSBS: 0010 0011
        // LSBS: 0100 1111
        // Pixel IDS from left to right:
        // 00, 01, 10, 00, 01, 01, 11, 11
        let bytes = LineBytes {
            msbs: 0x23,
            lsbs: 0x4f,
        };
        let line = TileLine::from_bytes(bytes);
        assert_eq!(line.color_ids, [Id0, Id1, Id2, Id0, Id1, Id1, Id3, Id3]);
        assert_eq!(line.as_bytes(), bytes);
        assert_eq!(line, TileLine::from_bytes(line.as_bytes()))
    }

    #[test]
    fn rw_vram_tile_data() {
        let initial_ppu = Ppu::new();
        assert_eq!(initial_ppu.read_vram_byte(0x8000), 0x00);
        assert_eq!(initial_ppu.read_vram_byte(0x8800), 0x00);
        assert_eq!(initial_ppu.read_vram_byte(0x9000), 0x00);
        for addr in [0x8000, 0x8800, 0x9000] {
            let mut ppu = initial_ppu.clone();
            let line_bytes = LineBytes {
                msbs: 0x23,
                lsbs: 0x4f,
            };
            ppu.write_vram_byte(addr, line_bytes.lsbs);
            ppu.write_vram_byte(addr + 1, line_bytes.msbs);
            assert_eq!(ppu.read_vram_byte(addr), line_bytes.lsbs);
            assert_eq!(ppu.read_vram_byte(addr + 1), line_bytes.msbs);
        }
    }

    #[test]
    fn rw_vram_tile_maps() {
        let initial_ppu = Ppu::new();
        assert_eq!(initial_ppu.read_vram_byte(0x9800), 0x00);
        assert_eq!(initial_ppu.read_vram_byte(0x9C00), 0x00);

        let byte = 0x4A;

        // Write to lo tile map [0][0]
        let mut ppu = initial_ppu.clone();
        ppu.write_vram_byte(0x9800, byte);
        assert_eq!(ppu.read_vram_byte(0x9800), byte);
        assert_eq!(ppu.lo_tile_map.tile_indices[0][0], byte);

        // Write to hi tile map [0][0]
        let mut ppu = initial_ppu.clone();
        ppu.write_vram_byte(0x9C00, byte);
        assert_eq!(ppu.read_vram_byte(0x9C00), byte);
        assert_eq!(ppu.hi_tile_map.tile_indices[0][0], byte);

        // Write to lo tile map [1][3]
        let addr = 0x9800 + 32 + 3;
        let mut ppu = initial_ppu.clone();
        ppu.write_vram_byte(addr, byte);
        assert_eq!(ppu.read_vram_byte(addr), byte);
        assert_eq!(ppu.lo_tile_map.tile_indices[1][3], byte);
    }
}
