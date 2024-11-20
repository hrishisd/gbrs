use anyhow::Context;
use std::path::PathBuf;
use std::thread;
use std::time::{self};

use enumset::EnumSet;
use sdl2::event::Event;
use sdl2::keyboard::Keycode;
use sdl2::pixels::PixelFormatEnum;

use clap::Parser;

use gbrs::joypad;
use gbrs::Color;

/// CPU frequency from pandocs: https://gbdev.io/pandocs/Specifications.html#dmg_clk
const CYCLES_PER_SECOND: u32 = 4194304;
const FPS: u32 = 60;
const CYCLES_PER_FRAME: u32 = CYCLES_PER_SECOND / FPS;
const NANOS_PER_FRAME: u64 = 1_000_000_000 / FPS as u64;
const FRAME_DURATION: time::Duration = time::Duration::from_nanos(NANOS_PER_FRAME);
use gbrs::mmu::Memory;

/// A Game Boy emulator
#[derive(Parser, Debug)]
#[command(version = "0", author = "Hrishi Dharam", about = "A Game Boy emulator")]
struct Cli {
    /// Path to the ROM file
    rom_path: PathBuf,

    /// Optional path to save state
    #[arg(long)]
    save: Option<PathBuf>,

    /// Don't sleep between frames (runs beyond 60 fps)
    #[arg(long, default_value = "false")]
    no_sleep: bool,

    /// Show the gameboy ppu window state in a separate window for debugging
    #[arg(long, default_value = "false")]
    show_window: bool,

    /// Render gameboy ppu background state in a separate window for debugging
    #[arg(long, default_value = "false")]
    show_bg: bool,

    /// Render gameboy object tiles in a separate window for debugging
    #[arg(long, default_value = "false")]
    show_obj_layer: bool,

    /// Vertical and horizontal scaling for the gameboy display
    #[arg(long, default_value = "4")]
    scale: u8,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Cli::parse();
    if args.scale == 0 {
        return Err("scale value must be > 0".into());
    }
    let rom = std::fs::read(&args.rom_path)
        .context(format!("Unable to read ROM: {:?}", args.rom_path))?;
    let emu = match &args.save {
        Some(sav_path) => {
            let sav = std::fs::read(sav_path)
                .context(format!("Unable to read sav file: {:?}", sav_path))?;
            gbrs::Emulator::load_save_state(&rom, sav_path, &sav)?
        }
        None => gbrs::Emulator::for_rom(&rom, &args.rom_path),
    };
    let sdl_context = sdl2::init()?;
    let video_subsystem = sdl_context.video()?;
    // bg layer
    let bg_canvas_and_texture = if args.show_bg {
        let window = video_subsystem
            .window(
                "Background Debug View",
                256 * args.scale as u32,
                256 * args.scale as u32,
            )
            .position(0, 0)
            .build()?;
        let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        canvas.set_scale(args.scale as f32, args.scale as f32)?;
        let texture_creator = Box::new(canvas.texture_creator());
        let texture_creator = Box::leak(texture_creator);
        let texture = texture_creator.create_texture_streaming(
            sdl2::pixels::PixelFormatEnum::RGB24,
            256,
            256,
        )?;
        Some((canvas, texture))
    } else {
        None
    };

    // window layer
    let window_canvas_and_texture = if args.show_window {
        let window = video_subsystem
            .window(
                "Window Debug View",
                256 * args.scale as u32,
                256 * args.scale as u32,
            )
            .position(512, 0)
            .build()?;
        let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        canvas.set_scale(args.scale as f32, args.scale as f32)?;
        let texture_creator = Box::new(canvas.texture_creator());
        let texture_creator = Box::leak(texture_creator);
        let texture = texture_creator
            .create_texture_streaming(sdl2::pixels::PixelFormatEnum::RGB24, 256, 256)
            .map_err(|e| e.to_string())?;
        Some((canvas, texture))
    } else {
        None
    };

    // object tiles layer
    let obj_canvas_and_texture = if args.show_obj_layer {
        let window = video_subsystem
            .window(
                "OAM Debug View",
                176 * args.scale as u32,
                176 * args.scale as u32,
            )
            .position(512, 100)
            .build()?;
        let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
        canvas.set_scale(args.scale as f32, args.scale as f32)?;
        let texture_creator = Box::new(canvas.texture_creator());
        let texture_creator = Box::leak(texture_creator);
        let texture = texture_creator.create_texture_streaming(
            sdl2::pixels::PixelFormatEnum::RGB24,
            176,
            176,
        )?;
        Some((canvas, texture))
    } else {
        None
    };

    let window = video_subsystem
        .window(
            "GB Emulator",
            160 * args.scale as u32,
            144 * args.scale as u32,
        )
        .position_centered()
        .build()
        .map_err(|e| e.to_string())?;
    let mut canvas = window.into_canvas().build().map_err(|e| e.to_string())?;
    canvas.set_scale(args.scale as f32, args.scale as f32)?;
    let event_pump = sdl_context.event_pump()?;
    let texture_creator = canvas.texture_creator();
    let texture = texture_creator.create_texture_streaming(PixelFormatEnum::RGB24, 160, 144)?;

    execute_rom(
        emu,
        event_pump,
        canvas,
        texture,
        bg_canvas_and_texture,
        window_canvas_and_texture,
        obj_canvas_and_texture,
        !args.no_sleep,
    )
}

#[allow(clippy::too_many_arguments)]
fn execute_rom(
    mut emu: gbrs::Emulator,
    mut event_pump: sdl2::EventPump,
    mut lcd_canvas: sdl2::render::Canvas<sdl2::video::Window>,
    mut lcd_texture: sdl2::render::Texture,
    mut background_canvas_and_texture: Option<(
        sdl2::render::Canvas<sdl2::video::Window>,
        sdl2::render::Texture,
    )>,
    mut window_canvas_and_texture: Option<(
        sdl2::render::Canvas<sdl2::video::Window>,
        sdl2::render::Texture,
    )>,
    mut obj_canvas_and_texture: Option<(
        sdl2::render::Canvas<sdl2::video::Window>,
        sdl2::render::Texture,
    )>,
    sleep_enabled: bool,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut pressed_buttons = EnumSet::<joypad::Button>::empty();
    let mut frame_count: u64 = 0;
    let mut print_logs: bool = false;
    let stdout = std::io::stdout();
    let mut lock = stdout.lock();
    let mut fast_mode = false;
    use std::io::Write;
    loop {
        let frame_start = std::time::Instant::now();
        // Handle events
        for event in event_pump.poll_iter() {
            match event {
                Event::Quit { .. } => return Ok(()),
                Event::KeyDown {
                    keycode: Some(key), ..
                } => {
                    if let Some(button) = keycode_to_button(key) {
                        pressed_buttons.insert(button);
                    } else if key == Keycode::D {
                        print_logs = true;
                    } else if key == Keycode::LShift {
                        fast_mode = true;
                    } else if key == Keycode::S {
                        match emu.dump_save_state() {
                            Ok(_) => {}
                            Err(e) => eprintln!("Failed to create save state: {e}"),
                        };
                    }
                }
                Event::KeyUp {
                    keycode: Some(key), ..
                } => {
                    if let Some(button) = keycode_to_button(key) {
                        pressed_buttons.remove(button);
                    } else if key == Keycode::D {
                        print_logs = false;
                    } else if key == Keycode::LShift {
                        fast_mode = false;
                    }
                }
                _ => {}
            };
        }
        emu.set_pressed_buttons(pressed_buttons);

        // Execute CPU cycles for one frame
        let mut cycles_in_frame: u32 = 0;
        while cycles_in_frame < CYCLES_PER_FRAME {
            let cycles = emu.step();
            cycles_in_frame += cycles as u32;

            if print_logs {
                // dump cpu state
                writeln!(lock, "CPU State:")?;
                writeln!(lock,
                "IME: {:?} HALTED: {:?}, IE: {:?}, IF: {:?}\nA:{:02X} F:{:02X} B:{:02X} C:{:02X} D:{:02X} E:{:02X} H:{:02X} L:{:02X} SP:{:04X} PC:{:04X} PCMEM:{:02X},{:02X},{:02X},{:02X}",
                emu.cpu.ime, emu.cpu.is_halted, emu.cpu.mmu.interrupts_enabled(), emu.cpu.mmu.interrupts_requested(), emu.cpu.regs.a, emu.cpu.regs.f, emu.cpu.regs.b, emu.cpu.regs.c, emu.cpu.regs.d, emu.cpu.regs.e, emu.cpu.regs.h, emu.cpu.regs.l, emu.cpu.regs.sp, emu.cpu.regs.pc, emu.cpu.mmu.read_byte(emu.cpu.regs.pc), emu.cpu.mmu.read_byte(emu.cpu.regs.pc+1), emu.cpu.mmu.read_byte(emu.cpu.regs.pc+2), emu.cpu.mmu.read_byte(emu.cpu.regs.pc+3))?;
                let ppu = emu.cpu.mmu.ppu_as_ref();
                writeln!(lock, "PPU State:")?;
                writeln!(lock, "  Mode: {:?}", ppu.mode)?;
                writeln!(lock, "  Line: {}", ppu.line)?;
                writeln!(lock, "  LCD Enabled: {}", ppu.lcd_enabled)?;
                writeln!(lock, "  Window Enabled: {}", ppu.window_enabled)?;
                writeln!(lock, "----------------------------------------")?;
            }
        }
        frame_count = frame_count.wrapping_add(1);

        let should_render = if fast_mode {
            frame_count % 5 == 0
        } else if !sleep_enabled {
            frame_count % 10 == 0
        } else {
            true
        };

        if should_render {
            // Update background texture
            if let Some((ref mut canvas, ref mut texture)) = background_canvas_and_texture {
                let background = emu.dbg_resolve_background();
                texture.with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                    for (y, row) in background.iter().enumerate() {
                        for (x, &color) in row.iter().enumerate() {
                            let offset = (y * background[0].len() + x) * 3;
                            let sdl_color = color_to_sdl_buf_values_dmg(color);
                            buffer[offset..offset + 3].copy_from_slice(&sdl_color);
                        }
                    }
                })?;
                canvas.clear();
                canvas.copy(texture, None, None)?;
                canvas.present();
            }

            // Update OAM texture
            if let Some((ref mut canvas, ref mut texture)) = obj_canvas_and_texture {
                let oam_data = emu.dbg_resolve_obj_layer();
                texture.with_lock(None, |buffer: &mut [u8], _pitch: usize| {
                    for (y, row) in oam_data.iter().enumerate() {
                        for (x, &color) in row.iter().enumerate() {
                            let offset = (y * oam_data[0].len() + x) * 3;
                            let sdl_color = color_to_sdl_buf_values_dmg(color);
                            buffer[offset..offset + 3].copy_from_slice(&sdl_color);
                        }
                    }
                })?;
                canvas.clear();
                canvas.copy(texture, None, None)?;
                canvas.present();
            }

            // update window texture
            if let Some((ref mut canvas, ref mut texture)) = window_canvas_and_texture {
                let window = emu.dbg_resolve_window();
                let window = window
                    .iter()
                    .map(|line| line.as_slice())
                    .collect::<Vec<_>>();
                update_canvas(canvas, texture, &window)?;
            }

            // update main display
            let lcd: [[Color; 160]; 144] = emu.resolve_display();
            let lcd: Vec<&[Color]> = lcd.iter().map(|line| line.as_slice()).collect();
            update_canvas(&mut lcd_canvas, &mut lcd_texture, &lcd)?;
        }

        // Sleep to maintain frame rate, if requested
        if sleep_enabled {
            let frame_duration = frame_start.elapsed();
            let frame_duration_wanted = if fast_mode {
                FRAME_DURATION / 10
            } else {
                FRAME_DURATION
            };
            if let Some(frame_remaining_duration) =
                frame_duration_wanted.checked_sub(frame_duration)
            {
                thread::sleep(frame_remaining_duration);
            }
        }
    }

    fn keycode_to_button(key: Keycode) -> Option<joypad::Button> {
        match key {
            Keycode::X => Some(joypad::Button::A),
            Keycode::Z => Some(joypad::Button::B),
            Keycode::Return => Some(joypad::Button::Start),
            Keycode::RShift => Some(joypad::Button::Select),
            Keycode::Up => Some(joypad::Button::Up),
            Keycode::Down => Some(joypad::Button::Down),
            Keycode::Left => Some(joypad::Button::Left),
            Keycode::Right => Some(joypad::Button::Right),
            _ => None,
        }
    }

    /// original Game Boy green
    #[inline(always)]
    fn color_to_sdl_buf_values_dmg(color: Color) -> [u8; 3] {
        static COLOR_LOOKUP: [[u8; 3]; 4] = [
            [224, 248, 208], // White
            [136, 192, 112], // LightGray
            [52, 104, 86],   // DarkGray
            [8, 24, 32],     // Black
        ];
        COLOR_LOOKUP[color as usize]
    }

    fn update_canvas(
        canvas: &mut sdl2::render::Canvas<sdl2::video::Window>,
        texture: &mut sdl2::render::Texture,
        image: &[&[Color]],
    ) -> Result<(), Box<dyn std::error::Error>> {
        texture.with_lock(None, |buffer: &mut [u8], _pitch: usize| {
            for (y, row) in image.iter().enumerate() {
                for (x, &color) in row.iter().enumerate() {
                    let offset = (y * image[0].len() + x) * 3;
                    let sdl_color = color_to_sdl_buf_values_dmg(color);
                    buffer[offset..offset + 3].copy_from_slice(&sdl_color);
                }
            }
        })?;
        canvas.clear();
        canvas.copy(texture, None, None)?;
        canvas.present();
        Ok(())
    }
}
