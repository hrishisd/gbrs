use std::thread;
use std::time::{self};

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

use gbrs::cpu;
use gbrs::Color;

/// CPU frequency from pandocs: https://gbdev.io/pandocs/Specifications.html#dmg_clk
const CYCLES_PER_SECOND: u32 = 4194304;
const FPS: u32 = 60;
const CYCLES_PER_FRAME: u32 = CYCLES_PER_SECOND / FPS;
const NANOS_PER_FRAME: u64 = 1_000_000_000 / FPS as u64;
const FRAME_DURATION: time::Duration = time::Duration::from_nanos(NANOS_PER_FRAME);

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Usage: cargo run -- rom_path
    // read the rom from the path provided
    // initialize the cpu with the rom
    // execute the rom
    // render the display at 60 frames per second
    let mut args = std::env::args();
    let _ = args.next();
    if let Some(rom_path) = args.next() {
        let rom = std::fs::read(rom_path)?;
        run_rom(&rom)
    } else {
        let rom = include_bytes!("../roms/Tetris (World) (Rev 1).gb");
        run_rom(rom)
    }
    // let rom_path = std::env::args()
    //     .nth(1)
    //     .expect("USAGE:\n\t<program> <rom_path>");
    // let file = std::fs::File::create("out.txt").unwrap();
    // let file = std::io::BufWriter::new(file);
    // let mut cpu = cpu::Cpu::_debug_mode(&rom, file);
    // let rom = include_bytes!("../roms/Tetris (World) (Rev 1).gb");
    // run_rom(rom)
}

fn run_rom(rom: &[u8]) -> Result<(), Box<dyn std::error::Error>> {
    let mut cpu = crate::cpu::Cpu::create(rom);
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("GB Emulator", 160 * 2, 144 * 2)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas.set_scale(2.0, 2.0)?;
    let mut event_pump = sdl_context.event_pump()?;
    let texture_creator = canvas.texture_creator();
    let mut texture = texture_creator
        .create_texture_streaming(PixelFormatEnum::RGB24, 160, 144)
        .map_err(|e| e.to_string())?;
    loop {
        let frame_start = std::time::Instant::now();
        // Execute CPU cycles for one frame
        let mut cycles_in_frame: u32 = 0;
        while cycles_in_frame < CYCLES_PER_FRAME {
            let cycles = cpu.step();
            cycles_in_frame += cycles as u32;
        }

        // Handle events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(()),
                Event::KeyDown {
                    keycode: Some(Keycode::Escape),
                    ..
                } => return Ok(()),
                _ => {}
            }
        }

        // Update the texture with the lcd_display data
        texture
            .with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                for (y, row) in cpu.mmu.ppu.lcd_display.iter().enumerate() {
                    for (x, &color) in row.iter().enumerate() {
                        let offset = (y * 160 + x) * 3;
                        let sdl_color = match color {
                            Color::White => [255, 255, 255],
                            Color::LightGray => [192, 192, 192],
                            Color::DarkGray => [96, 96, 96],
                            Color::Black => [0, 0, 0],
                        };
                        buffer[offset..offset + 3].copy_from_slice(&sdl_color);
                    }
                }
            })
            .map_err(|e| e.to_string())?;

        // Clear the canvas
        canvas.clear();

        // Copy the texture to the canvas
        canvas.copy(&texture, None, None)?;

        // Present the canvas
        canvas.present();

        // Sleep to maintain frame rate
        // let frame_duration = frame_start.elapsed();
        // if let Some(frame_remaining_duration) = FRAME_DURATION.checked_sub(frame_duration) {
        //     thread::sleep(frame_remaining_duration);
        // }
    }
}

// #[test]
// fn test_run_rom() {
//     let rom = include_bytes!("../roms/Tetris (World) (Rev 1).gb");
//     run_rom(rom).unwrap();
// }
