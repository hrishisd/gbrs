#![allow(incomplete_features)]
#![feature(assert_matches)]
#![feature(generic_const_exprs)]
mod cartridge;
pub mod cpu;
pub mod joypad;
pub mod mmu;
pub mod ppu;
use chrono;
mod timer;
mod util;
use anyhow::Context;
use std::{
    error::Error,
    path::{Path, PathBuf},
};
use twox_hash::xxh3;

use enumset::EnumSet;
use mmu::Memory;
pub use ppu::Color;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct Emulator {
    // TODO: make this private and make a pub function that returns debug info instead
    pub cpu: cpu::Cpu<mmu::Mmu>,
    rom_name: String,
    #[serde(skip)]
    save_dir: PathBuf,
    rom_hash: u64,
}

impl Emulator {
    pub fn for_rom(rom: &[u8], rom_path: &Path) -> Self {
        let rom_name = rom_path
            .file_stem()
            .and_then(|path| path.to_str())
            .expect("Illegal ROM file name")
            .to_string();
        let save_dir = rom_path
            .parent()
            .unwrap_or(Path::new("."))
            .join(&rom_name)
            .to_path_buf();
        eprintln!("Will put save files in {:?}", save_dir);
        let cpu = cpu::Cpu::new(mmu::Mmu::new(rom), false);
        Self {
            cpu,
            rom_name,
            save_dir,
            rom_hash: xxh3::hash64(rom),
        }
    }

    pub fn load_save_state(
        rom: &[u8],
        save_state_path: &Path,
        save_state: &[u8],
    ) -> Result<Self, Box<dyn Error>> {
        let save_state = zstd::decode_all(save_state)?;
        let mut emu: Emulator =
            rmp_serde::from_slice(&save_state).context("Error while deserializing emulator sav")?;
        if xxh3::hash64(rom) != emu.rom_hash {
            return Err("The provided ROM does not match the hash in the save state. This is not the correct ROM for the save.".into());
        }
        let save_dir = save_state_path
            .parent()
            .unwrap_or(Path::new("."))
            .to_path_buf();
        emu.save_dir = save_dir;
        emu.cpu.mmu.set_cart_rom(rom);
        Ok(emu)
    }

    pub fn dump_save_state(&self) -> Result<(), Box<dyn std::error::Error>> {
        // create save dir if it doesn't exist
        std::fs::create_dir_all(&self.save_dir).context("Failed to create save dir")?;
        let file_name = format!(
            "{}.sav.zst",
            chrono::Local::now().format("%Y-%m-%d-%H:%M:%S")
        );
        let save_file_path = self.save_dir.join(&file_name);
        eprintln!("Saving to {}", &file_name);
        let bytes = rmp_serde::to_vec(self)
            .context("Failed to serialize emulator state with message pack protocol")?;
        let compressed_bytes = zstd::encode_all(std::io::Cursor::new(&bytes), 0)
            .context("Failed to compress with zstd")?;
        std::fs::write(save_file_path, compressed_bytes)?;
        Ok(())
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
