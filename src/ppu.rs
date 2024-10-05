#[derive(Debug, Clone)]
pub struct Ppu {
    pub vram: [u8; 0x2000],
    /// There are 144 visible lines (0-143) and 10 additional invisible lines (144-153)
    pub line: u8,
    /// The number of cpu cycles spent in the current mode.
    ///
    /// Used to know when to switch modes and move the line index.
    cycles_in_mode: u32,
    mode: Mode,

    // -- LCD Control flags
    pub lcd_enabled: bool,
    pub window_tile_map_area: TileMapArea,
    pub window_enabled: bool,
    pub bg_and_window_data_tile_area: BgAndWindowTileDataArea,
    pub bg_tile_map_area: TileMapArea,
    pub obj_size: ObjSize,
    pub obj_enabled: bool,
    pub bg_enabled: bool,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
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

impl Ppu {
    pub(crate) fn new() -> Self {
        // TODO: check that enums are initialized to correct values
        Self {
            vram: [0; 0x2000],
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
                }
            }
            Mode::HorizontalBlank => {
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
