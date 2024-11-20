use std::assert_matches::assert_matches;

use enumset::EnumSet;
use serde::{Deserialize, Serialize};
use serde_big_array::BigArray;

use crate::{mmu::InterruptKind, util::U8Ext};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ppu {
    #[serde(skip, default = "DisplayLine::blank_display")]
    pub last_full_frame: [DisplayLine; 144],
    #[serde(skip, default = "DisplayLine::blank_display")]
    lcd_display: [DisplayLine; 144],
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
    pub cycles_in_mode: u32,
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
    #[serde(with = "BigArray")]
    pub obj_attribute_memory: [ObjectAttributes; 40],

    /// BGP
    ///
    /// Palette for background and window tiles.
    pub bg_color_palette: ColorPalette,
    /// The on-screen coordinates of the visible 160x144 pixel area within the 256x256 pixel background map.
    ///
    /// AKA SCY (ScrollY) and SCX (ScrollX)
    pub viewport_offset: Position,

    /// The on-screen coordinates of the window's top-left pixel (WY and WX)
    ///
    /// The x position of this coordinate is the actual x position of the window on the background - 7
    /// So if you want to draw the window in the upper left corner (0,0), this coordinate would be (0,7)
    /// The window is visible, if enabled, when x is in \[0,166\] and y is in \[0, 143\]
    pub window_top_left: Position,

    /// LCD Y compare. Used to set flags when compared with LY
    pub lyc: u8,
    /// LCD status register
    pub lcd_status: LcdStatus,
}

impl Ppu {
    pub(crate) fn new() -> Self {
        Self {
            vram_tile_data: VRamTileData {
                tile_data_blocks: [TileBlock(
                    [Tile {
                        lines: [TileLine { lsbs: 0, msbs: 0 }; 8],
                    }; 128],
                ); 3],
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
            viewport_offset: Position { x: 0, y: 0 },
            lyc: 0,
            lcd_status: LcdStatus {
                lyc_int_select: false,
                mode_2_int_select: false,
                mode_1_int_select: false,
                mode_0_int_select: false,
            },
            obj_color_palettes: [ColorPalette::from(0x00); 2],
            window_top_left: Position { x: 0, y: 0 },
            obj_attribute_memory: [ObjectAttributes {
                y_pos: 0,
                x_pos: 0,
                tile_idx: 0,
                bg_over_obj_priority: Priority::Zero,
                y_flip: false,
                x_flip: false,
                palette: ObjColorPaletteIdx::Zero,
            }; 40],
            lcd_display: [DisplayLine::black_line(); 144],
            last_full_frame: [DisplayLine::black_line(); 144],
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
                    &block.as_slice()[idx.tile_idx]
                };
                let line = tile.lines[idx.line_idx];
                if idx.byte_idx % 2 == 0 {
                    line.lsbs
                } else {
                    line.msbs
                }
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
                    &mut block.as_mut_slice()[idx.tile_idx]
                };
                let line = &mut tile.lines[idx.line_idx];
                if idx.byte_idx % 2 == 0 {
                    line.lsbs = byte;
                } else {
                    line.msbs = byte;
                }
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
                        self.last_full_frame = self.lcd_display;
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

    /// Draw a single scanline of the LCD display based on the current PPU state
    ///
    /// Returns an array of 160 colors representing one horizontal line of pixels
    ///
    /// # Arguments
    ///
    /// * `vram_tiles` - Tile data stored in VRAM
    /// * `line` - The current scanline being drawn (0-153)
    /// * `bg_and_window_tile_data_select` - Whether to use 0x8000 or 0x8800 addressing mode for background/window tiles
    /// * `bg_enabled` - Whether the background is enabled. This must be set for the window to be enabled
    /// * `bg_and_window_palette` - The color palette to use for background and window tiles
    /// * `bg_tile_map` - The tile map to use for background rendering
    /// * `bg_viewport_offset` - The viewport's offset within the background map (SCX/SCY)
    /// * `window_enabled` - Whether window rendering is enabled
    /// * `window_tile_map` - The tile map to use for window rendering
    /// * `window_top_left_pos` - The window's position on screen (WX,WY). The window's x coordinate on the LCD coordinate system is WX-7
    /// * `obj_enabled` - Whether sprite/object rendering is enabled
    /// * `obj_size` - Whether sprites are 8x8 or 8x16 pixels
    /// * `obj_attr_memory` - Object Attribute Memory containing sprite data
    /// * `obj_palettes` - The two color palettes available for sprites
    #[allow(clippy::too_many_arguments)]
    fn draw_scan_line_internal(
        // common args
        vram_tiles: &VRamTileData,
        lcd_line: u8,
        // bg and window common args
        bg_and_window_tile_data_select: BgAndWindowTileDataArea,
        bg_enabled: bool,
        bg_and_window_palette: ColorPalette,
        // background-specific args
        bg_tile_map: &TileMap,
        bg_viewport_offset: Position,
        // window-specific args
        window_enabled: bool,
        window_tile_map: &TileMap,
        window_top_left_pos: Position,
        // obj-specific args
        obj_enabled: bool,
        obj_size: ObjSize,
        obj_attr_memory: &[ObjectAttributes; 40],
        obj_palettes: [ColorPalette; 2],
    ) -> DisplayLine {
        let mut result = if bg_enabled {
            DisplayLine::black_line()
        } else {
            DisplayLine::white_line()
        };
        // Preserve the color ids while drawing the background and window to resolve priority when drawing objects
        let mut bg_line_color_ids = [ColorId::Id0; 160];
        if bg_enabled {
            // the index of the line being drawn in the 256x256 background coordinate system
            let bg_row: u8 = bg_viewport_offset.y.wrapping_add(lcd_line);
            for lcd_col in 0..160 {
                let bg_col: u8 = bg_viewport_offset.x.wrapping_add(lcd_col);
                // bg_row and bg_col represent the position of a pixel in the 256x256 background layer
                // Now we need to find the corresponding color id for this pixel in the background map
                let pixel_color_id = {
                    // First, find the corresponding tile for this pixel
                    let tile_idx =
                        bg_tile_map.tile_indices[bg_row as usize / 8][bg_col as usize / 8];
                    let tile = match bg_and_window_tile_data_select {
                        BgAndWindowTileDataArea::X8800 => {
                            vram_tiles.get_tile_from_0x8800_signed(tile_idx)
                        }
                        BgAndWindowTileDataArea::X8000 => vram_tiles.get_tile_from_0x8000(tile_idx),
                    };
                    tile.lines[bg_row as usize % 8].color_ids()[bg_col as usize % 8]
                };
                result.set_pixel(lcd_col, bg_and_window_palette.lookup(pixel_color_id));
                bg_line_color_ids[lcd_col as usize] = pixel_color_id;
            }
        }
        // the window is only visible if both the window and background are enabled, and the window offset falls within the ranges WX=0..166, WY=0..143
        let window_visible = bg_enabled
            && window_enabled
            && window_top_left_pos.y <= lcd_line
            && (0..=166).contains(&window_top_left_pos.x)
            && (0..=143).contains(&window_top_left_pos.y);
        if bg_enabled && window_visible {
            // the index of the line being drawn in the 256x256 window coordinate system
            let window_row = (lcd_line - window_top_left_pos.y) as usize;
            for lcd_col in 0..160 {
                // window_row, window_col are the index of a pixel in the 256x256 window coordinate system
                let window_col = lcd_col as i16 + 7 - window_top_left_pos.x as i16;
                if window_col < 0 {
                    // window is not visible at (line, lcd_col)
                    // (the window does not wrap around)
                } else {
                    let window_col = window_col as usize;
                    let pixel_color_id = {
                        let tile_idx = window_tile_map.tile_indices[window_row / 8][window_col / 8];
                        let tile = match bg_and_window_tile_data_select {
                            BgAndWindowTileDataArea::X8800 => {
                                vram_tiles.get_tile_from_0x8800_signed(tile_idx)
                            }
                            BgAndWindowTileDataArea::X8000 => {
                                vram_tiles.get_tile_from_0x8000(tile_idx)
                            }
                        };
                        tile.lines[window_row % 8].color_ids()[window_col % 8]
                    };
                    result.set_pixel(lcd_col, bg_and_window_palette.lookup(pixel_color_id));
                    bg_line_color_ids[lcd_col as usize] = pixel_color_id
                }
            }
        }
        if obj_enabled {
            // let mut objs_on_line = Vec::with_capacity(10);
            let obj_lines = |obj: ObjectAttributes| {
                let obj_lcd_y = obj.y_pos as i16 - 16;
                obj_lcd_y..(obj_lcd_y + obj_size.height() as i16)
            };
            // These are the (at-most) 10 objects on the line sorted from highest to lowest priority
            let prioritized_objects_on_line = {
                let mut objects_on_line = obj_attr_memory
                    .iter()
                    // filter only objects on line
                    .filter(|&&obj| obj_lines(obj).contains(&(lcd_line as i16)))
                    .take(10)
                    .collect::<Vec<_>>();
                objects_on_line.sort_by_key(|obj| obj.x_pos);
                objects_on_line
            };
            // draw the objects in lowest to highest priority, so that higher priority objects hide lower priority objects
            for obj in prioritized_objects_on_line.iter().rev() {
                // get the tile of this object that is on the current line

                // The index into an objects tile(s) of the line being rendered
                let obj_tiles_row_idx = {
                    // the y-position of the object on the lcd coordinate system
                    let obj_lcd_y = obj.y_pos as i16 - 16;
                    let line_idx = if obj.y_flip {
                        obj_size.height() as i16 - (lcd_line as i16 - obj_lcd_y) - 1
                    } else {
                        lcd_line as i16 - obj_lcd_y
                    };
                    assert_matches!(line_idx, 0..=15, "BUG: invalid result while calculating idx of object tile line, line: {}, obj: {:?}",lcd_line, obj);
                    line_idx as usize
                };

                // Get the row of the object's tiles that intersects with the
                let mut pixel_row = if obj_tiles_row_idx < 8 {
                    vram_tiles.get_tile_from_0x8000(obj.tile_idx).lines[obj_tiles_row_idx]
                } else {
                    assert!(obj_size == ObjSize::Dim8x16);
                    let base_tile_idx = obj.tile_idx & 0b1111_1110;
                    let tile = vram_tiles.get_tile_from_0x8000(base_tile_idx + 1);
                    tile.lines[obj_tiles_row_idx - 8]
                }
                .color_ids();

                if obj.x_flip {
                    pixel_row.reverse();
                }

                // Draw the line of the object tile
                for (pixel_idx, pixel_color_id) in pixel_row.into_iter().enumerate() {
                    // the position of this pixel on the LCD is
                    let lcd_col_idx = obj.x_pos as i16 - 8 + pixel_idx as i16;
                    // Only draw if this pixel of object appears on the display
                    if (0..160).contains(&lcd_col_idx) {
                        let lcd_col_idx = lcd_col_idx as u8;
                        let is_transparent = pixel_color_id == ColorId::Id0;
                        if !is_transparent
                            // Draw if the bg does not have priority over the object
                            && (obj.bg_over_obj_priority == Priority::Zero
                                || bg_line_color_ids[lcd_col_idx as usize] == ColorId::Id0)
                        {
                            let palette = obj_palettes[match obj.palette {
                                ObjColorPaletteIdx::Zero => 0,
                                ObjColorPaletteIdx::One => 1,
                            }];
                            result.set_pixel(lcd_col_idx, palette.lookup(pixel_color_id));
                        }
                    }
                }
            }
        }
        result
    }

    /// Resolve pixel values for a line of the LCD display
    fn draw_scan_line(&self) -> DisplayLine {
        Ppu::draw_scan_line_internal(
            &self.vram_tile_data,
            self.line,
            self.bg_and_window_tile_data_select,
            self.bg_enabled,
            self.bg_color_palette,
            match self.bg_tile_map_select {
                TileMapArea::X9800 => &self.lo_tile_map,
                TileMapArea::X9C00 => &self.hi_tile_map,
            },
            self.viewport_offset,
            self.window_enabled,
            match self.window_tile_map_select {
                TileMapArea::X9800 => &self.lo_tile_map,
                TileMapArea::X9C00 => &self.hi_tile_map,
            },
            self.window_top_left,
            self.obj_enabled,
            self.obj_size,
            &self.obj_attribute_memory,
            self.obj_color_palettes,
        )
    }

    /// This condition should be checked every time the current line is updated.
    fn should_trigger_lyc_interrupt(&self) -> bool {
        self.lcd_status.lyc_int_select && self.lyc == self.line
    }

    /// Construct a 256x256 grid of colors based on the ppu's background tile map and color palette.
    /// This returns the entire background and draws the viewport outline on the background
    /// This function ignores the background window enable bit.
    pub fn dbg_resolve_background(&self) -> [[Color; 256]; 256] {
        let mut background = [[Color::Black; 256]; 256];

        // Get the correct tile map based on bg_tile_map_select
        let tile_map = match self.bg_tile_map_select {
            TileMapArea::X9800 => &self.lo_tile_map,
            TileMapArea::X9C00 => &self.hi_tile_map,
        };

        // Iterate through each tile position in the 32x32 tile map
        for tile_y in 0..32 {
            for tile_x in 0..32 {
                // Get the tile index from the map
                let tile_idx = tile_map.tile_indices[tile_y][tile_x];

                // Get the actual tile based on bg_and_window_tile_data_select
                let tile = match self.bg_and_window_tile_data_select {
                    BgAndWindowTileDataArea::X8000 => {
                        self.vram_tile_data.get_tile_from_0x8000(tile_idx)
                    }
                    BgAndWindowTileDataArea::X8800 => {
                        self.vram_tile_data.get_tile_from_0x8800_signed(tile_idx)
                    }
                };

                // Each tile is 8x8 pixels
                // Calculate the starting pixel position in the background
                let start_x = tile_x * 8;
                let start_y = tile_y * 8;

                // Copy each pixel from the tile to the background
                for (line_idx, line) in tile.lines.iter().enumerate() {
                    for (pixel_idx, color_id) in line.color_ids().iter().enumerate() {
                        let bg_x = start_x + pixel_idx;
                        let bg_y = start_y + line_idx;
                        background[bg_y][bg_x] = self.bg_color_palette.lookup(*color_id);
                    }
                }
            }
        }

        // horizontal lines of viewport
        for i in 0..160 {
            let top_y = self.viewport_offset.y as usize;
            let bottom_y = (top_y + 144) % 256;
            let x = (self.viewport_offset.x as usize + i) % 256;
            background[top_y][x] = Color::Black;
            background[bottom_y][x] = Color::Black;
        }

        // vertical lines of viewport
        for i in 0..144 {
            let left_x = self.viewport_offset.x as usize;
            let right_x = (left_x + 160) % 256;
            let y = (self.viewport_offset.y as usize + i) % 256;
            background[y][left_x] = Color::Black;
            background[y][right_x] = Color::Black;
        }

        background
    }

    pub fn dbg_resolve_window(&self) -> [[Color; 256]; 256] {
        let mut window = [[Color::Black; 256]; 256];

        // Get the correct tile map based on window_tile_map_select
        let tile_map = match self.window_tile_map_select {
            TileMapArea::X9800 => &self.lo_tile_map,
            TileMapArea::X9C00 => &self.hi_tile_map,
        };

        // Iterate through each tile position in the 32x32 tile map
        for tile_y in 0..32 {
            for tile_x in 0..32 {
                // Get the tile index from the map
                let tile_idx = tile_map.tile_indices[tile_y][tile_x];

                // Get the actual tile based on bg_and_window_tile_data_select
                let tile = match self.bg_and_window_tile_data_select {
                    BgAndWindowTileDataArea::X8000 => {
                        self.vram_tile_data.get_tile_from_0x8000(tile_idx)
                    }
                    BgAndWindowTileDataArea::X8800 => {
                        self.vram_tile_data.get_tile_from_0x8800_signed(tile_idx)
                    }
                };

                // Each tile is 8x8 pixels
                // Calculate the starting pixel position in the window
                let start_x = tile_x * 8;
                let start_y = tile_y * 8;

                // Copy each pixel from the tile to the window
                for (line_idx, line) in tile.lines.iter().enumerate() {
                    for (pixel_idx, color_id) in line.color_ids().iter().enumerate() {
                        let window_x = start_x + pixel_idx;
                        let window_y = start_y + line_idx;
                        window[window_y][window_x] = self.bg_color_palette.lookup(*color_id);
                    }
                }
            }
        }
        window
    }

    /// Draw the objects in the object attribute memory as a grid of pixels
    /// The objects appear on their own grid.
    /// The 0,0 of the object grid corresponds to -8, -16 of the lcd coordinate system
    pub fn dbg_resolve_objects(&self) -> [[Color; 176]; 176] {
        let mut grid = [[Color::White; 176]; 176];
        // an objects y position (obj.y_pos) is its position on the lcd screen + 16
        // So for example
        // Y=0 hides an object,
        // Y=2 hides an 8×8 object but displays the last two rows of an 8×16 object,
        // Y=16 displays an object at the top of the screen,
        // Y=144 displays an 8×16 object aligned with the bottom of the screen,
        // Y=152 displays an 8×8 object aligned with the bottom of the screen,
        // Y=154 displays the first six rows of an object at the bottom of the screen,
        // Y=160 hides an object.

        // an objects x position (obj.x_pos) is its horizontal position on the lcd screen + 8
        // an off screen value of x = 0 or x >= 168 hides the object
        for obj in self.obj_attribute_memory {
            let mut tile = self
                .vram_tile_data
                .get_tile_from_0x8000(obj.tile_idx)
                .lines
                .map(|line| line.color_ids());
            if obj.x_flip {
                for line in tile.iter_mut() {
                    line.reverse();
                }
            }
            if obj.y_flip {
                tile.reverse();
            }
            for (y_offset, line) in tile.iter().enumerate() {
                for (x_offset, color_id) in line.iter().enumerate() {
                    let x = obj.x_pos as usize + x_offset;
                    let y = obj.y_pos as usize + y_offset;
                    // don't draw objects out of frame
                    if x < 176 && y < 176 {
                        let palette = self.obj_color_palettes[match obj.palette {
                            ObjColorPaletteIdx::Zero => 0,
                            ObjColorPaletteIdx::One => 1,
                        }];
                        let pixel = palette.lookup(*color_id);
                        grid[y][x] = pixel;
                    }
                }
            }
        }
        // draw vertical lines of lcd
        #[allow(clippy::needless_range_loop)]
        for y in 16..=160 {
            grid[y][8] = Color::Black;
            grid[y][168] = Color::Black;
        }
        // draw horizontal lines of lcd
        for x in 8..=168 {
            grid[16][x] = Color::Black;
            grid[160][x] = Color::Black;
        }
        grid
    }
}

/// A packed representation of the colors within a line
/// Each byte represents 4 pixels
/// The 0th byte represents the 4 left-most pixels
/// The two left-most bits of the 0th byte represent the color of the first pixel
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct DisplayLine(#[serde(with = "BigArray")] [u8; 40]);

impl Default for DisplayLine {
    fn default() -> Self {
        DisplayLine([0; 40])
    }
}

impl std::fmt::Debug for DisplayLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_list()
            .entries((0..160).map(|idx| self.pixel_at(idx)))
            .finish()
    }
}

impl DisplayLine {
    fn blank_display() -> [DisplayLine; 144] {
        [DisplayLine::black_line(); 144]
    }

    pub fn colors(&self) -> [Color; 160] {
        let mut result = [Color::White; 160];
        for idx in 0..160 {
            result[idx as usize] = self.pixel_at(idx);
        }
        result
    }

    pub fn pixel_at(&self, idx: u8) -> Color {
        assert_matches!(
            idx,
            0..=159,
            "Out of range idx while indexing into display line: {idx}"
        );
        let byte_idx = idx >> 2;
        let byte = (&self.0)[byte_idx as usize];

        let idx_of_color_in_byte = 3 - idx % 4;
        let color_bits = (byte >> (2 * idx_of_color_in_byte)) & 0b11;
        Color::from_be_bits([color_bits.bit(1), color_bits.bit(0)])
    }

    fn set_pixel(&mut self, idx: u8, color: Color) {
        assert_matches!(
            idx,
            0..=159,
            "Out of range idx while indexing into display line: {idx}"
        );
        let byte_idx = idx >> 2;
        let mut byte = (&self.0)[byte_idx as usize];

        // the index of the color within the byte from left to right
        // idx%4 of 0 means we want the left-most pixel in the byte, with 2-bit-idx of 3
        // idx%4 of 3 means we want the right-most pixel in the byte, with 2-bit-idx of 0
        let color_idx = 3 - (idx % 4);

        // clear the corresponding color of the byte
        byte &= 0b1111_1100u8.rotate_left(2 * color_idx as u32);
        // updat the color at that position in the byte
        let color_mask = (color as u8) << (2 * color_idx);
        byte |= color_mask;
        (&mut self.0)[byte_idx as usize] = byte
    }

    fn black_line() -> Self {
        DisplayLine([0xFF; 40])
    }
    fn white_line() -> Self {
        DisplayLine([0x00; 40])
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

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    pub x: u8,
    pub y: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileMap {
    pub tile_indices: [[u8; 32]; 32],
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

    fn height(self) -> u8 {
        match self {
            ObjSize::Dim8x8 => 8,
            ObjSize::Dim8x16 => 16,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum Color {
    White = 0,
    LightGray = 1,
    DarkGray = 2,
    Black = 3,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColorPalette(Color, Color, Color, Color);

impl ColorPalette {
    pub fn lookup(&self, id: ColorId) -> Color {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
struct TileBlock(#[serde(with = "BigArray")] [Tile; 128]);

impl TileBlock {
    fn as_slice(&self) -> &[Tile] {
        &self.0
    }

    fn as_mut_slice(&mut self) -> &mut [Tile] {
        &mut self.0
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VRamTileData {
    #[serde(with = "BigArray")]
    tile_data_blocks: [TileBlock; 3],
}

impl VRamTileData {
    /// Read a tile from blocks 0 or 1, using unsigned addressing.
    ///
    /// idx 0 to 127 gets from block 0
    ///
    /// idx 128 to 255 gets from block 1
    pub fn get_tile_from_0x8000(&self, idx: u8) -> Tile {
        if idx < 128 {
            self.tile_data_blocks[0].as_slice()[idx as usize]
        } else {
            self.tile_data_blocks[1].as_slice()[idx as usize % 128]
        }
    }

    /// Read a tile from blocks 1 or 2 using signed addressing
    ///
    /// idx 0 to 127 searches within block 2
    ///
    /// idx -1 to -128 searches within block 1
    pub fn get_tile_from_0x8800_signed(&self, idx: u8) -> Tile {
        let idx = idx as i8;
        if idx >= 0 {
            self.tile_data_blocks[2].as_slice()[idx as usize]
        } else {
            self.tile_data_blocks[1].as_slice()[(idx as i16 + 128) as usize]
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tile {
    /// `lines[0]` is the top-line
    pub lines: [TileLine; 8],
}

/// In both lsbs and msbs, bit 7 represents the left-most pixel and bit 0, the right-most
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TileLine {
    pub lsbs: u8,
    pub msbs: u8,
}

impl TileLine {
    /// idx 0 represents the left-most pixel, idx 7 is the right-most pixel
    pub fn color_ids(self) -> [ColorId; 8] {
        let mut result = [ColorId::Id0; 8];
        for bit_idx in 0..=7 {
            result[7 - bit_idx as usize] = match (self.msbs.bit(bit_idx), self.lsbs.bit(bit_idx)) {
                (false, false) => ColorId::Id0,
                (false, true) => ColorId::Id1,
                (true, false) => ColorId::Id2,
                (true, true) => ColorId::Id3,
            }
        }
        result
    }

    /// idx 0 in the array represents the left-most pixel, idx 7 is the right-most pixel
    pub fn from_color_ids(color_ids: [ColorId; 8]) -> Self {
        let (mut lsbs, mut msbs) = (0, 0);
        for (idx, color_id) in color_ids.iter().enumerate() {
            let bit_idx = 7 - idx as u8;
            match color_id {
                ColorId::Id0 => {}
                ColorId::Id1 => {
                    lsbs = lsbs.set(bit_idx);
                }
                ColorId::Id2 => {
                    msbs = msbs.set(bit_idx);
                }
                ColorId::Id3 => {
                    lsbs = lsbs.set(bit_idx);
                    msbs = msbs.set(bit_idx);
                }
            }
        }
        TileLine { lsbs, msbs }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColorId {
    Id0,
    Id1,
    Id2,
    Id3,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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
    pub bg_over_obj_priority: Priority,
    pub y_flip: bool,
    pub x_flip: bool,
    pub palette: ObjColorPaletteIdx,
}

impl ObjectAttributes {
    pub fn as_bytes(&self) -> [u8; 4] {
        let byte_3 = u8::from_bits([
            match self.bg_over_obj_priority {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjColorPaletteIdx {
    Zero,
    One,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Priority {
    Zero,
    One,
}

#[cfg(test)]
mod tests {

    use proptest::{prop_assert_eq, proptest};

    use super::*;

    proptest! {
        #[test]
        fn display_line_roundtrip(color_id in 0..4, pixel_idx in 0..160u8) {
            let colors = [
                Color::White,
                Color::LightGray,
                Color::DarkGray,
                Color::Black,
            ];
            let mut line = DisplayLine::black_line();
            let color = colors[color_id as usize % 4];
            line.set_pixel(pixel_idx, color);
            prop_assert_eq!(line.pixel_at(pixel_idx), color);
            prop_assert_eq!(&line.colors()[..pixel_idx as usize], &vec![Color::Black;pixel_idx as usize]);
            prop_assert_eq!(&line.colors()[pixel_idx as usize+1..], &vec![Color::Black;160-pixel_idx as usize-1])
        }
    }

    #[test]
    fn display_line_round_trip() {
        use Color::*;
        let mut line = DisplayLine::black_line();
        line.set_pixel(0, LightGray);
        line.set_pixel(1, DarkGray);
        line.set_pixel(2, White);
        assert_eq!(line.pixel_at(0), LightGray);
        assert_eq!(line.pixel_at(1), DarkGray);
        assert_eq!(line.pixel_at(2), White);
    }

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
    fn rw_vram_tile_data() {
        let initial_ppu = Ppu::new();
        assert_eq!(initial_ppu.read_vram_byte(0x8000), 0x00);
        assert_eq!(initial_ppu.read_vram_byte(0x8800), 0x00);
        assert_eq!(initial_ppu.read_vram_byte(0x9000), 0x00);
        for addr in [0x8000, 0x8800, 0x9000] {
            let mut ppu = initial_ppu.clone();
            let line = TileLine {
                msbs: 0x23,
                lsbs: 0x4f,
            };
            ppu.write_vram_byte(addr, line.lsbs);
            ppu.write_vram_byte(addr + 1, line.msbs);
            assert_eq!(ppu.read_vram_byte(addr), line.lsbs);
            assert_eq!(ppu.read_vram_byte(addr + 1), line.msbs);
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
            lines: [TileLine::from_color_ids([color_id; 8]); 8],
        }
    }

    #[test]
    fn draw_bg_only() {
        let mut ppu = Ppu::new();

        ppu.bg_enabled = true;
        ppu.window_enabled = false;
        ppu.obj_enabled = false;
        ppu.viewport_offset = Position { x: 0, y: 0 };
        ppu.line = 0;
        ppu.bg_and_window_tile_data_select = BgAndWindowTileDataArea::X8000;
        ppu.bg_tile_map_select = TileMapArea::X9800;
        // Create 4 tiles in VRAM in block 0 with different color IDs
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[0] = mono_color_tile(ColorId::Id0);
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[1] = mono_color_tile(ColorId::Id1);
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[2] = mono_color_tile(ColorId::Id2);
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[3] = mono_color_tile(ColorId::Id3);
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
        assert_eq!(lcd_row.colors()[..8], [Color::White; 8]);
        assert_eq!(lcd_row.colors()[8..], [Color::LightGray; 152]);

        // move the viewport to the right by 1 pixel
        ppu.viewport_offset.x = 1;
        // Now the first row of the LCD should be 7 white pixels followed by 152 light gray pixels
        let lcd_row = ppu.draw_scan_line();
        assert_eq!(lcd_row.colors()[..7], [Color::White; 7]);
        assert_eq!(lcd_row.colors()[7..], [Color::LightGray; 153]);

        // we should get the same line even as we scroll the viewport down up to line 7, because each row of tiles 1 and 2 is identical
        for y_offset in 1..7 {
            let lcd_row = ppu.draw_scan_line();
            ppu.viewport_offset.y = y_offset;
            assert_eq!(lcd_row.colors()[..7], [Color::White; 7]);
            assert_eq!(lcd_row.colors()[7..], [Color::LightGray; 153]);
        }

        // now fill the second tile row in the background map with 1 dark gray tile followed by 31 black tiles
        ppu.lo_tile_map.tile_indices[1][0] = 2;
        ppu.lo_tile_map.tile_indices[1][1..].fill(3);
        ppu.viewport_offset = Position { x: 0, y: 3 };
        ppu.line = 5;
        // we are now drawing line 5 of the LCD screen, which is offset 3 from the top of the background map
        // This should display the second row of tiles
        let lcd_row = ppu.draw_scan_line();
        assert_eq!(lcd_row.colors()[..8], [Color::DarkGray; 8]);
        assert_eq!(lcd_row.colors()[8..], [Color::Black; 152]);
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
                    TileLine::from_color_ids([Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id0, Id0, Id0, Id0, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id3, Id3, Id3, Id3, Id2, Id2, Id2, Id2]),
                ],
            }
        };
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[0] = obj_tile;
        ppu.obj_attribute_memory[0] = ObjectAttributes {
            y_pos: 0,
            x_pos: 0,
            tile_idx: 0,
            bg_over_obj_priority: Priority::Zero,
            y_flip: false,
            x_flip: false,
            palette: ObjColorPaletteIdx::Zero,
        };
        // first, at position 0,0, the object should be invisible
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors(), [Color::White; 160]);

        // now, make the object visible by moving it down 9 rows and to the right 1 column
        ppu.obj_attribute_memory[0].y_pos = 9;
        ppu.obj_attribute_memory[0].x_pos = 1;
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[0], Color::DarkGray);
        // The rest of the screen should still be blank
        assert_eq!(line.colors()[1..], [Color::White; 159]);
        ppu.line = 1;
        assert_eq!(ppu.draw_scan_line().colors(), [Color::White; 160]);

        // Now, flip the object vertically and render the last line of the object on the first line of the lcd
        ppu.line = 0;
        ppu.obj_attribute_memory[0].x_pos = 8;
        ppu.obj_attribute_memory[0].y_pos = 9;
        ppu.obj_attribute_memory[0].y_flip = true;

        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[..4], [Color::White; 4]);
        assert_eq!(line.colors()[4..8], [Color::LightGray; 4]);
        assert_eq!(line.colors()[8..], [Color::White; 152]);

        // Now flip the object horizontally and vertically and render the last line of the object
        ppu.obj_attribute_memory[0].x_flip = true;
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[..4], [Color::LightGray; 4]);
        assert_eq!(line.colors()[4..], [Color::White; 156]);

        // Now unflip the object vertically and render the last line of the object
        ppu.obj_attribute_memory[0].y_flip = false;
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[..4], [Color::DarkGray; 4]);
        assert_eq!(line.colors()[4..8], [Color::Black; 4]);
        assert_eq!(line.colors()[8..], [Color::White; 152]);
    }

    #[test]
    fn draw_8x16_obj() {
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
                    TileLine::from_color_ids([Id1, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                    TileLine::from_color_ids([Id2, Id2, Id2, Id2, Id2, Id2, Id2, Id2]),
                ],
            }
        };
        // The second tile should have dark gray in the top-left pixel and light gray everywhere else
        let light_tile = {
            use ColorId::*;
            // let mut tile = mono_color_tile(Id1);
            // tile.lines[0] =
            //     TileLineCompact::from_color_ids([Id2, Id1, Id1, Id1, Id1, Id1, Id1, Id1]);

            Tile {
                lines: [
                    TileLine::from_color_ids([Id2, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                    TileLine::from_color_ids([Id1, Id1, Id1, Id1, Id1, Id1, Id1, Id1]),
                ],
            }
        };

        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[0] = dark_tile;
        ppu.vram_tile_data.tile_data_blocks[0].as_mut_slice()[1] = light_tile;
        ppu.obj_attribute_memory[0] = ObjectAttributes {
            y_pos: 0,
            x_pos: 0,
            tile_idx: 0,
            bg_over_obj_priority: Priority::Zero,
            y_flip: false,
            x_flip: false,
            palette: ObjColorPaletteIdx::Zero,
        };
        // first, at position 0,0, the object should be invisible
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors(), [Color::White; 160]);

        // now, make the object visible by moving it down a single row row and to the right 8 columns
        ppu.obj_attribute_memory[0].y_pos = 1;
        ppu.obj_attribute_memory[0].x_pos = 8;
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[..8], [Color::LightGray; 8]);
        // The rest of the screen should still be black
        assert_eq!(line.colors()[8..], [Color::White; 152]);
        ppu.line = 1;
        assert_eq!(ppu.draw_scan_line().colors(), [Color::White; 160]);

        // Now flip the object vertically and rerender the first line of the object
        ppu.obj_attribute_memory[0].y_flip = true;
        ppu.line = 0;

        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[0], Color::LightGray);
        assert_eq!(line.colors()[1..8], [Color::DarkGray; 7]);
        assert_eq!(line.colors()[8..], [Color::White; 152]);

        // Now flip the object horizontally and vertically and render the first line of the object
        ppu.obj_attribute_memory[0].x_flip = true;
        let line = ppu.draw_scan_line();
        assert_eq!(line.colors()[..7], [Color::DarkGray; 7]);
        assert_eq!(line.colors()[7], Color::LightGray);
        assert_eq!(line.colors()[8..], [Color::White; 152]);

        // Now, keep the object flipped vertically, move it fully into the screen, and check its rendered correctly
        ppu.obj_attribute_memory[0].y_flip = true;
        ppu.obj_attribute_memory[0].x_flip = false;
        ppu.obj_attribute_memory[0].y_pos = 16;
        ppu.line = 0;
        let top_line = ppu.draw_scan_line();
        assert_eq!(top_line.colors()[..8], [Color::LightGray; 8]);
        ppu.line = 7;
        let first_tile_bottom_line = ppu.draw_scan_line();
        assert_eq!(first_tile_bottom_line.pixel_at(0), Color::DarkGray);
        assert_eq!(first_tile_bottom_line.colors()[1..8], [Color::LightGray; 7]);
        ppu.line = 8;
        let second_tile_top_line = ppu.draw_scan_line();
        assert_eq!(second_tile_top_line.colors()[..8], [Color::DarkGray; 8]);
        ppu.line = 15;
        let second_tile_bottom_line = ppu.draw_scan_line();
        assert_eq!(second_tile_bottom_line.pixel_at(0), Color::LightGray);
        assert_eq!(second_tile_bottom_line.colors()[1..8], [Color::DarkGray; 7]);
    }
}
