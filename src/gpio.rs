//#[cfg(feature = "use_rppal")]
use rppal;

use std::error::Error;

pub fn gpio_common_new(pin_number:u8) ->  Result<Box<GpioCommon>, Box<Error>> {
    let g = GpioCommonRppal::new(pin_number)?;
    Ok(Box::new(g))
}

/// Common interface for GPIO operations
/// Possibly it would help to use different gpio libraries
/// Unfortunately right now only one library are compatible with my code.
///
pub trait GpioCommon {
    /// Configure pin as pull up input
    fn mode_input(&mut self);
    /// Configure pin to output mode
    fn mode_output(&mut self);
    /// Set pin output true (high) or false (low) level
    fn set(&mut self, level: bool);
    /// Set high pin output
    fn high(&mut self);
    /// Set low pin output
    fn low(&mut self);
    /// Read true (high) or false (low) level from input
    fn read(&mut self) -> Result<bool, Box<Error>>;
    /// Reset pin to original state that was at wrapper initalization
    fn reset(&mut self);
}

//#[cfg(feature = "use_rppal")]
struct GpioCommonRppal {
    pin: u8,
    init_mode: rppal::gpio::Mode,
    rppal: rppal::gpio::Gpio,
}

//#[cfg(feature = "use_rppal")]
impl GpioCommonRppal {
    fn new(pin: u8) -> Result<GpioCommonRppal, Box<Error>> {
        let mut pp = rppal::gpio::Gpio::new()?;
        let mode = pp.mode(pin)?;

        Ok(GpioCommonRppal {
            rppal: pp,
            pin: pin,
            init_mode: mode,
        })
    }
}

//#[cfg(feature = "use_rppal")]
impl GpioCommon for GpioCommonRppal {
    fn mode_input(&mut self) {
        self.rppal.set_mode(self.pin, rppal::gpio::Mode::Input);
    }

    fn mode_output(&mut self) {
        self.rppal.set_mode(self.pin, rppal::gpio::Mode::Output);
    }

    fn set(&mut self, level: bool) {
        if level {
            self.rppal.write(self.pin, rppal::gpio::Level::High)
        } else {
            self.rppal.write(self.pin, rppal::gpio::Level::Low)
        }
    }

    fn high(&mut self) {
        self.set(true);
    }

    fn low(&mut self) {
        self.set(false);
    }

    fn read(&mut self) -> Result<bool, Box<Error>> {
        let res = self.rppal.read(self.pin)?;
        Ok(match res {
            rppal::gpio::Level::High => true,
            rppal::gpio::Level::Low => false,
        })
    }

    fn reset(&mut self) {
        self.rppal.set_mode(self.pin, self.init_mode);
    }
}