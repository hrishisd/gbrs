use core::panic;
use std::assert_matches::assert_matches;

use enumset::EnumSet;

use crate::{mmu::InterruptKind, util::U8Ext};

#[derive(Debug, Clone)]
pub struct Ppu {
    pub lcd_display: [[Color; 160]; 144],
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
    pub window_tile_map_select: TileMapArea,
    // Draw the window only when this bit is set
    pub window_enabled: bool,
    pub bg_and_window_tile_data_select: BgAndWindowTileDataArea,
    pub bg_tile_map_select: TileMapArea,
    /// color idx 0 is always transparent for objs.
    ///
    /// There are 2 color palettes so that the game can use all 4 available colors for objects.
    pub obj_color_palettes: [ColorPalette; 2],
    pub obj_size: ObjSize,
    /// Draw objects only when this bit is set
    pub obj_enabled: bool,
    // Draw the background only when this bit is set
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
    /// AKA SCY (ScrollY) and SCX (ScrollX)
    pub bg_viewport_offset: Coord,

    /// The on-screen coordinates of the window's top-left pixel (WY and WX)
    ///
    /// The x position of this coordinate is the actual x position of the window on the background - 7
    /// So if you want to draw the window in the upper left corner (0,0), this coordinate would be (0,7)
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
            window_tile_map_select: TileMapArea::from_bit(false),
            window_enabled: false,
            bg_and_window_tile_data_select: BgAndWindowTileDataArea::X8800,
            bg_tile_map_select: TileMapArea::from_bit(false),
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
            lcd_display: [[Color::Black; 160]; 144],
        }
    }

    pub(crate) fn read_vram_byte(&self, addr: u16) -> u8 {
        // Tile ID is the middle 2 bytes of the address
        match addr {
            // Tiles
            0x8000..=0x97FF => {
                let idx = TileByteIdx::from_addr(addr);
                let tile = {
                    let block = &self.vram_tile_data.tile_data_blocks[idx.block_idx];
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

                    // Now GPU has finished drawing the line, write it to the LCD
                    if self.line < 144 {
                        self.lcd_display[self.line as usize] = self.draw_scan_line();
                    }
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

    /// Resolve pixel values for a line of the LCD display
    fn draw_scan_line(&self) -> [Color; 160] {
        let mut lcd_line = [Color::Black; 160];
        let mut lcd_line_bg_and_window_color_ids = [ColorId::Id0; 160];
        if self.bg_enabled {
            let tile_map = match self.bg_tile_map_select {
                TileMapArea::X9800 => &self.lo_tile_map,
                TileMapArea::X9C00 => &self.hi_tile_map,
            };
            // The index of the line being rendered, in reference to the entire 256x256 background
            let bg_y_pos = self.bg_viewport_offset.y.wrapping_add(self.line);
            for lcd_x_pos in 0u8..160 {
                let bg_x_pos = self.bg_viewport_offset.x.wrapping_add(lcd_x_pos);

                // we are resolving the value of the pixel on the lcd at (lcd_x_pos, self.line)
                // This is equivalent to resolving the value of the pixel on the background at (bg_x_pos, bg_y_pos)
                let tile_idx = tile_map.tile_indices[bg_y_pos as usize / 8][bg_x_pos as usize / 8];
                let tile = match self.bg_and_window_tile_data_select {
                    BgAndWindowTileDataArea::X8000 => {
                        self.vram_tile_data.get_tile_from_0x8000(tile_idx)
                    }
                    BgAndWindowTileDataArea::X8800 => {
                        self.vram_tile_data.get_tile_from_0x8800_signed(tile_idx)
                    }
                };

                let tile_line_idx = bg_y_pos % 8;
                let tile_col_idx = bg_x_pos % 8;
                let color_id = tile.lines[tile_line_idx as usize].color_ids[tile_col_idx as usize];
                let color = self.bg_color_palette.lookup(color_id);
                lcd_line[lcd_x_pos as usize] = color;
                lcd_line_bg_and_window_color_ids[lcd_x_pos as usize] = color_id;
            }
        }
        if self.obj_enabled {
            let obj_height = match self.obj_size {
                ObjSize::Dim8x8 => 8,
                ObjSize::Dim8x16 => 16,
            };
            for obj in self.obj_attribute_memory {
                // range of lcd lines that the object occupies
                // The position of the object on the lcd's coordinate system
                let obj_lcd_y_pos = obj.y_pos as i16 - 16;
                let obj_lcd_x_pos = obj.x_pos as i16 - 8;
                let obj_visible_on_line = (1..168).contains(&obj.x_pos)
                    && ((obj_lcd_y_pos)..(obj_lcd_y_pos + obj_height))
                        .contains(&(self.line as i16));
                if !obj_visible_on_line {
                    continue;
                }

                // The index of the tile line of the object that is on this lcd line
                let obj_line_idx = if obj.y_flip {
                    obj_height - (self.line as i16 - obj_lcd_y_pos) - 1
                } else {
                    self.line as i16 - obj_lcd_y_pos
                };

                let obj_row = {
                    let line = if obj_line_idx <= 7 {
                        self.vram_tile_data.get_tile_from_0x8000(obj.tile_idx).lines
                            [obj_line_idx as usize]
                    } else {
                        assert_eq!(obj_height, 16);
                        self.vram_tile_data
                            .get_tile_from_0x8000(obj.tile_idx + 1)
                            .lines[(obj_line_idx - 8) as usize]
                    };
                    if obj.x_flip {
                        let mut clone = line.color_ids.clone();
                        clone.reverse();
                        clone
                    } else {
                        line.color_ids
                    }
                };
                for (pixel_color_idx, pixel_color_id) in obj_row.iter().enumerate() {
                    // the index of this pixel in the lcd line
                    let lcd_idx = obj_lcd_x_pos + pixel_color_idx as i16;
                    let is_transparent = *pixel_color_id == ColorId::Id0;
                    if lcd_idx >= 0
                        && lcd_idx < 160
                        && !is_transparent
                        // check should render over background
                        && (obj.priority == Priority::Zero
                            || lcd_line_bg_and_window_color_ids[lcd_idx as usize] == ColorId::Id0 || !self.bg_enabled)
                    {
                        let palette = self.obj_color_palettes[match obj.palette {
                            ObjColorPaletteIdx::Zero => 0,
                            ObjColorPaletteIdx::One => 1,
                        }];
                        lcd_line[lcd_idx as usize] = palette.lookup(*pixel_color_id);
                    }
                }
            }
        }
        if self.window_enabled {
            let window_tile_map = match self.window_tile_map_select {
                TileMapArea::X9800 => &self.lo_tile_map,
                TileMapArea::X9C00 => &self.hi_tile_map,
            };
            let window_y = self.line - self.window_top_left.y;
            for window_x in 0u8..160 {
                let lcd_x_pos = window_x.wrapping_add(self.window_top_left.x.wrapping_sub(7));
                if lcd_x_pos >= 160 {
                    continue;
                }
                let tile_x = window_x / 8;
                let tile_y = window_y / 8;
                let tile_idx = window_tile_map.tile_indices[tile_y as usize][tile_x as usize];
                let tile = match self.bg_and_window_tile_data_select {
                    BgAndWindowTileDataArea::X8000 => {
                        self.vram_tile_data.get_tile_from_0x8000(tile_idx)
                    }
                    BgAndWindowTileDataArea::X8800 => {
                        self.vram_tile_data.get_tile_from_0x8800_signed(tile_idx)
                    }
                };
                let tile_line_idx = window_y % 8;
                let tile_col_idx = window_x % 8;
                let color_id = tile.lines[tile_line_idx as usize].color_ids[tile_col_idx as usize];
                let color = self.bg_color_palette.lookup(color_id);
                lcd_line[lcd_x_pos as usize] = color;
                lcd_line_bg_and_window_color_ids[lcd_x_pos as usize] = color_id;
            }
        }

        lcd_line
    }

    /// This condition should be checked every time the current line is updated.
    fn should_trigger_lyc_interrupt(&self) -> bool {
        self.lcd_status.lyc_int_select && self.lyc == self.line
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileByteIdx {
    /// The index of the block in the vram tile data
    block_idx: usize,
    /// The index of the tile in the block
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
                let tile_idx = (addr as usize >> 4) % 128;
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
pub enum Color {
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

impl ColorPalette {
    fn lookup(&self, id: ColorId) -> Color {
        match id {
            ColorId::Id0 => self.0,
            ColorId::Id1 => self.1,
            ColorId::Id2 => self.2,
            ColorId::Id3 => self.3,
        }
    }
}

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
pub enum Mode {
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
    /// idx 0 to 127 gets from block 0
    ///
    /// idx 128 to 255 gets from block 1
    fn get_tile_from_0x8000(&self, idx: u8) -> Tile {
        if idx < 128 {
            self.tile_data_blocks[0][idx as usize]
        } else {
            self.tile_data_blocks[1][idx as usize % 128]
        }
    }

    /// Read a tile from blocks 1 or 2 using signed addressing
    ///
    /// idx 0 to 127 searches within block 2
    ///
    /// idx -1 to -128 searches within block 1
    fn get_tile_from_0x8800_signed(&self, idx: u8) -> Tile {
        let idx = idx as i8;
        if idx >= 0 {
            self.tile_data_blocks[2][idx as usize]
        } else {
            self.tile_data_blocks[1][(idx as i16 + 128) as usize]
        }
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
    /// Object’s vertical position on the screen + 16.
    ///
    /// E.g:
    ///
    /// Y=0 hides an object
    ///
    /// Y=2 hides an 8×8 object but displays the last two rows of an 8×16 object.
    pub y_pos: u8,
    /// Object’s horizontal position on the screen + 8.
    ///
    /// An off-screen value (X=0 or X>=168) hides the object.
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
    fn tile_idx_calculation() {
        // Test first tile in block 0
        assert_eq!(
            TileByteIdx::from_addr(0x8000),
            TileByteIdx {
                block_idx: 0,
                tile_idx: 0,
                byte_idx: 0,
                line_idx: 0,
            }
        );
        // Test block 0
        assert_eq!(
            TileByteIdx::from_addr(0x8490),
            TileByteIdx {
                block_idx: 0,
                tile_idx: 0x49,
                byte_idx: 0,
                line_idx: 0,
            }
        );
        // Test block 1
        assert_eq!(
            TileByteIdx::from_addr(0x8B80),
            TileByteIdx {
                block_idx: 1,
                tile_idx: 0x38,
                byte_idx: 0,
                line_idx: 0,
            }
        );
        // Test block 2
        assert_eq!(
            TileByteIdx::from_addr(0x95A0),
            TileByteIdx {
                block_idx: 2,
                tile_idx: 0x5A,
                byte_idx: 0,
                line_idx: 0,
            }
        );
    }

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

    fn mono_color_tile(color_id: ColorId) -> Tile {
        Tile {
            lines: [TileLine {
                color_ids: [color_id; 8],
            }; 8],
        }
    }

    #[test]
    fn draw_bg_only() {
        let mut ppu = Ppu::new();

        ppu.bg_enabled = true;
        ppu.window_enabled = false;
        ppu.obj_enabled = false;
        ppu.bg_viewport_offset = Coord { x: 0, y: 0 };
        ppu.line = 0;
        ppu.bg_and_window_tile_data_select = BgAndWindowTileDataArea::X8000;
        ppu.bg_tile_map_select = TileMapArea::X9800;
        // Create 4 tiles in VRAM in block 0 with different color IDs
        ppu.vram_tile_data.tile_data_blocks[0][0] = mono_color_tile(ColorId::Id0);
        ppu.vram_tile_data.tile_data_blocks[0][1] = mono_color_tile(ColorId::Id1);
        ppu.vram_tile_data.tile_data_blocks[0][2] = mono_color_tile(ColorId::Id2);
        ppu.vram_tile_data.tile_data_blocks[0][3] = mono_color_tile(ColorId::Id3);
        ppu.bg_color_palette = ColorPalette(
            Color::White,     // Tile 0
            Color::LightGray, // Tile 1
            Color::DarkGray,  // Tile 2
            Color::Black,     // Tile 3
        );

        // fill the first two rows of the background map
        // The first row is a white tile followed by 31 light gray tiles
        ppu.lo_tile_map.tile_indices[0][1] = 0;
        ppu.lo_tile_map.tile_indices[0][1..].fill(1);

        // The first row of the LCD should be 8 white pixels followed by 152 light gray pixels
        let lcd_row = ppu.draw_scan_line();
        assert_eq!(lcd_row[..8], [Color::White; 8]);
        assert_eq!(lcd_row[8..], [Color::LightGray; 152]);

        // move the viewport to the right by 1 pixel
        ppu.bg_viewport_offset.x = 1;
        // Now the first row of the LCD should be 7 white pixels followed by 152 light gray pixels
        let lcd_row = ppu.draw_scan_line();
        assert_eq!(lcd_row[..7], [Color::White; 7]);
        assert_eq!(lcd_row[7..], [Color::LightGray; 153]);

        // we should get the same line even as we scroll the viewport down up to line 7, because each row of tiles 1 and 2 is identical
        for y_offset in 1..7 {
            let lcd_row = ppu.draw_scan_line();
            ppu.bg_viewport_offset.y = y_offset;
            assert_eq!(lcd_row[..7], [Color::White; 7]);
            assert_eq!(lcd_row[7..], [Color::LightGray; 153]);
        }

        // now fill the second tile row in the background map with 1 dark gray tile followed by 31 black tiles
        ppu.lo_tile_map.tile_indices[1][0] = 2;
        ppu.lo_tile_map.tile_indices[1][1..].fill(3);
        ppu.bg_viewport_offset = Coord { x: 0, y: 3 };
        ppu.line = 5;
        // we are now drawing line 5 of the LCD screen, which is offset 3 from the top of the background map
        // This should display the second row of tiles
        let lcd_row = ppu.draw_scan_line();
        assert_eq!(lcd_row[..8], [Color::DarkGray; 8]);
        assert_eq!(lcd_row[8..], [Color::Black; 152]);
    }

    #[test]
    fn draw_obj_only() {
        let mut ppu = Ppu::new();
        ppu.bg_enabled = false;
        ppu.window_enabled = false;
        ppu.obj_enabled = true;
        ppu.line = 0;
        ppu.obj_size = ObjSize::Dim8x8;
        ppu.obj_color_palettes[0] = ColorPalette(
            Color::White, // transparent
            Color::LightGray,
            Color::DarkGray,
            Color::Black,
        );

        // Make an 8x8 object that is 4 blocks of 4x4 tiles, so that we can test flips
        //  [transparent] [light gray]
        //  [black] [dark gray]
        let obj_tile = {
            use ColorId::*;
            Tile {
                lines: [
                    TileLine {
                        color_ids: [Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2],
                    },
                ],
            }
        };
        ppu.vram_tile_data.tile_data_blocks[0][0] = obj_tile;
        ppu.obj_attribute_memory[0] = ObjectAttributes {
            y_pos: 0,
            x_pos: 0,
            tile_idx: 0,
            priority: Priority::Zero,
            y_flip: false,
            x_flip: false,
            palette: ObjColorPaletteIdx::Zero,
        };
        // first, at position 0,0, the object should be invisible
        let line = ppu.draw_scan_line();
        assert_eq!(line, [Color::Black; 160]);

        // now, make the object visible by moving it down 9 rows and to the right 1 column
        ppu.obj_attribute_memory[0].y_pos = 9;
        ppu.obj_attribute_memory[0].x_pos = 1;
        let line = ppu.draw_scan_line();
        assert_eq!(line[0], Color::DarkGray);
        // The rest of the screen should still be black
        assert_eq!(line[1..], [Color::Black; 159]);
        ppu.line = 1;
        assert_eq!(ppu.draw_scan_line(), [Color::Black; 160]);

        // Now, flip the object vertically and render the last line of the object on the first line of the lcd
        ppu.line = 0;
        ppu.obj_attribute_memory[0].x_pos = 8;
        ppu.obj_attribute_memory[0].y_pos = 9;
        ppu.obj_attribute_memory[0].y_flip = true;

        let line = ppu.draw_scan_line();
        assert_eq!(line[..4], [Color::Black; 4]);
        assert_eq!(line[4..8], [Color::LightGray; 4]);
        assert_eq!(line[8..], [Color::Black; 152]);

        // Now flip the object horizontally and vertically and render the last line of the object
        ppu.obj_attribute_memory[0].x_flip = true;
        let line = ppu.draw_scan_line();
        assert_eq!(line[..4], [Color::LightGray; 4]);
        assert_eq!(line[4..], [Color::Black; 156]);

        // Now unflip the object vertically and render the last line of the object
        ppu.obj_attribute_memory[0].y_flip = false;
        let line = ppu.draw_scan_line();
        assert_eq!(line[..4], [Color::DarkGray; 4]);
        assert_eq!(line[4..], [Color::Black; 156]);
    }

    #[test]
    fn draw_stacked_obj() {
        let mut ppu = Ppu::new();
        ppu.bg_enabled = false;
        ppu.window_enabled = false;
        ppu.obj_enabled = true;
        ppu.line = 0;
        ppu.obj_size = ObjSize::Dim8x16;
        ppu.obj_color_palettes[0] = ColorPalette(
            Color::White, // transparent
            Color::LightGray,
            Color::DarkGray,
            Color::Black,
        );

        // Make two 8x8 object tiles
        // The first tile should have light gray in the top-left pixel and dark gray everywhere else
        let dark_tile = {
            use ColorId::*;
            Tile {
                lines: [
                    TileLine {
                        color_ids: [Id1, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                    TileLine {
                        color_ids: [Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2],
                    },
                ],
            }
        };
        // The second tile should have dark gray in the top-left pixel and light gray everywhere else
        let light_tile = {
            use ColorId::*;
            Tile {
                lines: [
                    TileLine {
                        color_ids: [Id2, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                    TileLine {
                        color_ids: [Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1],
                    },
                ],
            }
        };

        ppu.vram_tile_data.tile_data_blocks[0][0] = dark_tile;
        ppu.vram_tile_data.tile_data_blocks[0][1] = light_tile;
        ppu.obj_attribute_memory[0] = ObjectAttributes {
            y_pos: 0,
            x_pos: 0,
            tile_idx: 0,
            priority: Priority::Zero,
            y_flip: false,
            x_flip: false,
            palette: ObjColorPaletteIdx::Zero,
        };
        // first, at position 0,0, the object should be invisible
        let line = ppu.draw_scan_line();
        assert_eq!(line, [Color::Black; 160]);

        // now, make the object visible by moving it down a single row row and to the right 8 columns
        ppu.obj_attribute_memory[0].y_pos = 1;
        ppu.obj_attribute_memory[0].x_pos = 8;
        let line = ppu.draw_scan_line();
        assert_eq!(line[..8], [Color::LightGray; 8]);
        // The rest of the screen should still be black
        assert_eq!(line[8..], [Color::Black; 152]);
        ppu.line = 1;
        assert_eq!(ppu.draw_scan_line(), [Color::Black; 160]);

        // Now flip the object vertically and rerender the first line of the object
        ppu.obj_attribute_memory[0].y_flip = true;
        ppu.line = 0;

        let line = ppu.draw_scan_line();
        assert_eq!(line[0], Color::LightGray);
        assert_eq!(line[1..8], [Color::DarkGray; 7]);
        assert_eq!(line[8..], [Color::Black; 152]);

        // Now flip the object horizontally and vertically and render the first line of the object
        ppu.obj_attribute_memory[0].x_flip = true;
        let line = ppu.draw_scan_line();
        assert_eq!(line[..7], [Color::DarkGray; 7]);
        assert_eq!(line[7], Color::LightGray);
        assert_eq!(line[8..], [Color::Black; 152]);

        // Now, keep the object flipped vertically, move it fully into the screen, and check its rendered correctly
        ppu.obj_attribute_memory[0].y_flip = true;
        ppu.obj_attribute_memory[0].x_flip = false;
        ppu.obj_attribute_memory[0].y_pos = 16;
        ppu.line = 0;
        let top_line = ppu.draw_scan_line();
        assert_eq!(top_line[..8], [Color::LightGray; 8]);
        ppu.line = 7;
        let first_tile_bottom_line = ppu.draw_scan_line();
        assert_eq!(first_tile_bottom_line[0], Color::DarkGray);
        assert_eq!(first_tile_bottom_line[1..8], [Color::LightGray; 7]);
        ppu.line = 8;
        let second_tile_top_line = ppu.draw_scan_line();
        assert_eq!(second_tile_top_line[..8], [Color::DarkGray; 8]);
        ppu.line = 15;
        let second_tile_bottom_line = ppu.draw_scan_line();
        assert_eq!(second_tile_bottom_line[0], Color::LightGray);
        assert_eq!(second_tile_bottom_line[1..8], [Color::DarkGray; 7]);
    }
}
