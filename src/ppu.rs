use crate::util::U8Ext;

#[derive(Debug, Clone)]
pub struct Ppu {
    pub vram: [u8; 0x2000],
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
    pub bg_and_window_data_tile_area: BgAndWindowTileDataArea,
    pub bg_tile_map_area: TileMapArea,
    /// color idx 0 is always transparent for objs
    pub obj_color_palettes: [ColorPalette; 2],
    pub obj_size: ObjSize,
    pub obj_enabled: bool,
    pub bg_enabled: bool,

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
            vram: [0; 0x2000],
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
            bg_and_window_data_tile_area: BgAndWindowTileDataArea::from_bit(false),
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
                lyc_eq_lq: false,
            },
            obj_color_palettes: [ColorPalette::from(0x00); 2],
            window_top_left: Coord { x: 0, y: 0 },
        }
    }

    pub(crate) fn step(&mut self, t_cycles: u8) {
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

                    // Now GPU has finished drawing the line, write it to the frame buffer
                }
            }
            Mode::HorizontalBlank => {
                assert!(self.line < 144);
                if self.cycles_in_mode >= 204 {
                    self.cycles_in_mode -= 204;
                    self.line += 1;
                    if self.line == 144 {
                        self.mode = Mode::VerticalBlank;
                    } else {
                        assert!(self.line < 144);
                        self.mode = Mode::ScanlineOAM;
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
                }
            }
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub struct LcdStatus {
    ///  If set, selects the LYC == LY condition for the STAT interrupt
    pub lyc_int_select: bool,
    /// If set, selects teh Mode 2 condition for the STAT interrupt
    pub mode_2_int_select: bool,
    /// If set, selects teh Mode 1 condition for the STAT interrupt
    pub mode_1_int_select: bool,
    /// If set, selects teh Mode 0 condition for the STAT interrupt
    pub mode_0_int_select: bool,
    /// (Read-only) Set when LY contains the same value as LYC
    pub lyc_eq_lq: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coord {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TileMap {
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
    pub fn from_bit(b: bool) -> Self {
        if b {
            BgAndWindowTileDataArea::X8000
        } else {
            BgAndWindowTileDataArea::X8800
        }
    }

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
    /// Takes 172 to 289 clock cycles depending on the amount of assets being rendered
    ScanlineVRAM,
    /// Can take between 87 to 204 cycles, depending on how long mode `ScanlineVRAM` took.
    HorizontalBlank,
    /// Once the last visible row (143) has been processed, there are 10 additional rows which take 4560 clock cycles to process.
    ///
    /// After that, we go back to row 0.
    VerticalBlank,
}

struct VRamTileData {
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
enum ColorId {
    Id0,
    Id1,
    Id2,
    Id3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct TileLine {
    color_ids: [ColorId; 8],
}

impl TileLine {
    /// For each line, the first byte specifies the least significant bit of the color ID of each pixel, and the second byte specifies the most significant bit.
    /// In both bytes, bit 7 represents the leftmost pixel, and bit 0 the rightmost. For example, the tile data $3C $7E $42 $42 $42 $42 $42 $42 $7E $5E $7E $0A $7C $56 $38 $7C appears as follows:
    fn as_bytes(&self) -> (u8, u8) {
        todo!()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Tile {
    lines: [TileLine; 8],
}
