#![allow(incomplete_features)]
#![feature(assert_matches)]
#![feature(generic_const_exprs)]
pub mod cpu;
pub mod joypad;
mod mmu;
pub mod ppu;
mod timer;
mod util;
pub use ppu::Color;

// TODO: consider making a type emulator that owns cpu, gpu, and mmu, and have cpu, gpu and mmu take mutable references to eachother
