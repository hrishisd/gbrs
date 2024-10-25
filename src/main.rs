use std::path::PathBuf;
use std::thread;
use std::time::{self};

use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

use clap::Parser;

use gbrs::cpu;
use gbrs::Color;

/// CPU frequency from pandocs: https://gbdev.io/pandocs/Specifications.html#dmg_clk
const CYCLES_PER_SECOND: u32 = 4194304;
const SPEED_MOD: u32 = 4;
const FPS: u32 = 120;
const CYCLES_PER_FRAME: u32 = SPEED_MOD * CYCLES_PER_SECOND / FPS;
const NANOS_PER_FRAME: u64 = 1_000_000_000 / FPS as u64;
const FRAME_DURATION: time::Duration = time::Duration::from_nanos(NANOS_PER_FRAME);

/// A Game Boy emulator
#[derive(Parser, Debug)]
#[command(
    version = "0",
    author = "Hrishi Dharam",
    about = "My Game Boy emulator"
)]
struct Cli {
    /// Path to the ROM file
    rom: PathBuf,

    /// Print CPU logs to stdout
    #[arg(long, default_value = "false")]
    stdout_logs: bool,

    /// Log CPU state to a file
    #[arg(long, value_name = "FILE")]
    log_file: Option<PathBuf>,

    /// Don't sleep between frames to force 60 fps
    #[arg(long, default_value = "false")]
    no_sleep: bool,

    /// Initialize the CPU as if the boot ROM executed successfully
    #[arg(long, default_value = "false")]
    skip_boot: bool,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    let rom = std::fs::read(args.rom)?;
    let log_file = if let Some(log_file) = args.log_file {
        let log = std::fs::File::create(log_file)?;
        let log = std::io::BufWriter::new(log);
        Some(log)
    } else {
        None
    };
    let cpu: cpu::Cpu = if args.skip_boot {
        cpu::Cpu::new_post_boot(&rom, log_file, args.stdout_logs)
    } else {
        cpu::Cpu::new(&rom, log_file, args.stdout_logs)
    };
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    let window = video_subsystem
        .window("GB Emulator", 160 * 2, 144 * 2)
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;

    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas.set_scale(2.0, 2.0)?;
    let event_pump = sdl_context.event_pump()?;
    let texture_creator = canvas.texture_creator();
    let texture = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, 160, 144)?;
    execute_rom(cpu, event_pump, canvas, texture, !args.no_sleep)
}

fn execute_rom(
    mut cpu: cpu::Cpu,
    mut event_pump: sdl2::EventPump,
    mut canvas: sdl2::render::Canvas<sdl2::video::Window>,
    mut texture: sdl2::render::Texture,
    should_sleep: bool,
) -> Result<(), Box<dyn std::error::Error>> {
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
                // TODO: handle other key press events
                _ => {}
            }
        }

        // Update the texture with the lcd_display data
        // then copy the texture to the canvas and present it
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
        canvas.clear();
        canvas.copy(&texture, None, None)?;
        canvas.present();

        // Sleep to maintain frame rate, if requested
        if should_sleep {
            let frame_duration = frame_start.elapsed();
            if let Some(frame_remaining_duration) = FRAME_DURATION.checked_sub(frame_duration) {
                thread::sleep(frame_remaining_duration);
            }
        }
    }
}
