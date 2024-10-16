#![feature(assert_matches)]
pub mod cpu;
mod mmu;
mod ppu;
mod timer;
mod util;

// TODO: consider making a type emulator that owns cpu, gpu, and mmu, and have cpu, gpu and mmu take mutable references to eachother
