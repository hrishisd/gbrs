#![allow(incomplete_features)]
#![feature(assert_matches)]
#![feature(generic_const_exprs)]
mod cartridge;
pub mod cpu;
pub mod joypad;
mod mmu;
pub mod ppu;
mod timer;
mod util;
use std::path::{Path, PathBuf};

use enumset::EnumSet;
pub use ppu::Color;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Emulator {
    // TODO: make this private and make a pub function that returns debug info instead
    pub cpu: cpu::Cpu,
    rom_name: String,
    save_state_id: usize,
    #[serde(skip)]
    save_dir: PathBuf,
}

impl Emulator {
    pub fn for_rom(rom: &[u8], rom_path: &PathBuf) -> Self {
        let rom_name = rom_path
            .file_stem()
            .and_then(|path| path.to_str())
            .expect("Illegal ROM file name")
            .to_string();
        let save_dir = rom_path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let cpu = cpu::Cpu::new(&rom, false);
        Self {
            cpu,
            rom_name,
            save_state_id: 0,
            save_dir,
        }
    }

    pub fn load_save_state(
        save_state: &[u8],
        save_state_path: &PathBuf,
    ) -> Result<Self, serde_json::Error> {
        let mut emu: Emulator = serde_json::from_slice(&save_state)?;
        let save_dir = save_state_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        emu.save_dir = save_dir;
        Ok(emu)
    }

    /// Fetch, decode, and execute a single instruction.
    ///
    /// Returns the number of master clock cycles (at 4 MiHz) that the instruction takes. E.g. executing the NOP instruction will return 4
    pub fn step(&mut self) -> u8 {
        self.cpu.step()
    }

    pub fn set_pressed_buttons(&mut self, pressed: EnumSet<joypad::Button>) {
        self.cpu.mmu.set_pressed_buttons(pressed);
    }

    pub fn resolve_display(&self) -> [[Color; 160]; 144] {
        let display = self.cpu.mmu.ppu_as_ref().lcd_display;
        display.map(|line| line.colors())
    }

    pub fn dbg_resolve_window(&self) -> [[Color; 256]; 256] {
        self.cpu.mmu.ppu_as_ref().dbg_resolve_window()
    }

    pub fn dbg_resolve_background(&self) -> [[Color; 256]; 256] {
        self.cpu.mmu.ppu_as_ref().dbg_resolve_background()
    }

    pub fn dbg_resolve_obj_layer(&self) -> [[Color; 176]; 176] {
        self.cpu.mmu.ppu_as_ref().dbg_resolve_objects()
    }
}
