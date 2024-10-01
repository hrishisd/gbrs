use rand::{self, Rng};
use std::{
    io::{stdout, Write},
    thread::{self},
    time::{self, Duration, Instant},
};

const OFF_COLOR_CODE: i32 = 232;
const ON_COLOR_CODE: i32 = 214;

mod cpu;
mod mmu;

fn test_rendering() {
    // generate random 160 wide x 144 tall grid of bools
    let mut rng = rand::thread_rng();
    let mut displays: Vec<[[bool; 160]; 144]> = Vec::new();
    let start = Instant::now();
    for _ in 0..1_000 {
        displays.push([[false; 160]; 144]);
        for row in 0..144 {
            for col in 0..160 {
                let last_idx = displays.len() - 1;
                displays[last_idx][row][col] = rng.gen_bool(0.5);
            }
        }
    }
    let elapsed = start.elapsed();
    let mut times = vec![];
    println!("Generated displays in {:.2?}", elapsed);
    println!("switching to alternate screen");
    thread::sleep(Duration::from_secs(2));

    // switch to alternate screen buffer
    print!("\x1b[?1049h");
    // hide the cursor before rendering
    print!("\x1b[?25l");
    stdout().flush().unwrap();
    thread::sleep(Duration::from_secs(5));

    for display in displays {
        let display_string = generate_display_string(display);
        let start = Instant::now();
        print!("{}", display_string);
        stdout().flush().unwrap();
        times.push(start.elapsed());
    }
    println!("Pausing for a bit to observe scroll back");
    thread::sleep(time::Duration::from_secs(5));

    // Show the cursor
    print!("\x1b[?25h");
    // Switch back to the normal screen buffer
    print!("\x1b[?1049l");
    stdout().flush().unwrap();
    println!("Switched back to normal screen buffer");
    stdout().flush().unwrap();
    let total_render_time: Duration = times.iter().sum();
    println!("total render time: {total_render_time:?}");
    println!(
        "avg render time per frame: {:?}",
        total_render_time / times.len() as u32
    );
    println!("max frame render time: {:?}", times.iter().max());
}

// Generate a string, that when printed in raw mode, draws the display to the terminal window
fn generate_display_string(display: [[bool; 160]; 144]) -> String {
    assert!(
        display.len() % 2 == 0,
        "Expected an even number of rows in the display, got {}",
        display.len()
    );
    let mut output = String::new();
    // Erase from cursor to beginning of screen
    // output.push_str("\x1b[1J");
    // Move the cursor to the top-left corner
    output.push_str("\x1b[H");

    let lower_half_block = '▄';
    let upper_half_block = '▀';
    let full_block = '█';
    // set the background color
    output.push_str(format!("\x1b[48;5;{}m", OFF_COLOR_CODE).as_str());
    // set the foreground color
    output.push_str(format!("\x1b[38;5;{}m", ON_COLOR_CODE).as_str());
    for row_idx in (0..display.len()).step_by(2) {
        for col_idx in 0..display[0].len() {
            let top_pixel = display[row_idx][col_idx];
            let bottom_pixel = display[row_idx + 1][col_idx];
            if top_pixel && bottom_pixel {
                output.push(full_block)
            } else if top_pixel {
                output.push(upper_half_block);
            } else if bottom_pixel {
                output.push(lower_half_block);
            } else {
                output.push(' ');
            }
        }
        output.push('\n');
        output.push('\r');
    }
    output
}

fn main() {
    // experiment();
    test_rendering();
}

#[test]
fn test_generate_display_string() {
    println!("Printing from here:");
    let display = [[true; 160]; 144];
    let display_string = generate_display_string(display);
    println!("{}", display_string);
}
