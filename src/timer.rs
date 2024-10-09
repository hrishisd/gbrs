#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::enum_variant_names)]
pub enum TimerFrequency {
    F4KiHz,
    F16KiHz,
    F64KiHz,
    F256KiHz,
}

impl TimerFrequency {
    /// The timer's frequency can be expressed as the number of system clock cycles (T-cycles) per tick of the timer.
    ///
    /// The system clock runs at 4 MiHZ, so we divide the system clock frequency by the timer frequency to get the number of clock cycles per timer tick.
    fn t_cycles_per_tick(self) -> u16 {
        use TimerFrequency::*;
        match self {
            F4KiHz => 1024,
            F16KiHz => 256,
            F64KiHz => 64,
            F256KiHz => 16,
        }
    }
}

pub struct Timer {
    pub frequency: TimerFrequency,
    pub enabled: bool,
    /// Timer modulo.
    ///
    /// When the timer overflows, it is reset to the value in this register.
    pub tma: u8,
    pub value: u8,
    /// The number of t-cycles since the last tick of the timer
    t_cycles_count: u16,
}

impl Timer {
    pub fn new(frequency: TimerFrequency) -> Self {
        Timer {
            frequency,
            enabled: false,
            tma: 0,
            value: 0,
            t_cycles_count: 0,
        }
    }

    /// Update the state of the timer by simulating `tCycles` T-cycles and return whether the timer overflowed.
    pub fn update(&mut self, t_cycles: u8) -> bool {
        if !self.enabled {
            return false;
        }

        self.t_cycles_count += t_cycles as u16;
        if self.t_cycles_count > self.frequency.t_cycles_per_tick() {
            self.value = self.value.wrapping_add(1);
            if self.value == 0 {
                self.value = self.tma;
                return true;
            }
        }
        false
    }
}
