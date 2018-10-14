/**
 * Common GPIO interface.
 * 
 * @see https://github.com/torvalds/linux/blob/v4.4/include/linux/gpio/consumer.h
 * @see https://www.kernel.org/doc/Documentation/gpio/consumer.txt
 */

//#[cfg(feature = "use_rppal")]
use rppal;

use std::io::{Error,ErrorKind};
use std::error::Error as ErrorStd;

pub fn gpio_pin_new(pin_number: u32) -> Result<Box<GpioPin>, Error> {
    let g = GpioPinRppal::new(pin_number)?;
    Ok(Box::new(g))
}

/// This trait represents single GPIO pin for spinlock-Safe GPIO Access
/// Most GPIO controllers can be accessed with memory read/write instructions. Those
/// don't need to sleep, and can safely be done from inside hard (non-threaded) IRQ handlers and similar contexts.
pub trait GpioPin {
    
    /// Setting pin direction as input without activation of any pull up/down resitors.
    /// 
    /// Keep in mind that get/set calls don't return errors and since misconfiguration is possible.
    fn direction_input(&mut self) -> Result<(), Error>;
    
    /// Setting pin direction as output.
    /// 
    /// For output GPIOs, the value provided becomes the initial output value.
    /// This helps avoid signal glitching during system startup.
    /// 
    /// Keep in mind that get/set calls don't return errors and since misconfiguration is possible.
    fn direction_output(&mut self, value: i32) -> Result<(), Error>;
    
    /// Set pin value.
    /// The values are boolean, zero for low, nonzero for high. 
    /// The get/set calls do not return errors because "invalid GPIO" should have been reported earlier from gpiod_direction_*(). 
    /// Also, using these calls for GPIOs that can't safely be accessed without sleeping (see below) is an error.
    fn set(&mut self, value: i32);

    /// Read pin value.
    /// The values are boolean, zero for low, nonzero for high. 
    /// When reading the value of an output pin, the value returned should be what's seen on the pin. 
    /// That won't always match the specified output value, because of issues including open-drain signaling and output latencies.
    /// The get/set calls do not return errors because "invalid GPIO" should have been reported earlier from direction_*(). 
    /// However, note that not all platforms can read the value of output pins; those that can't should always return zero.
    /// Also, using these calls for GPIOs that can't safely be accessed without sleeping (see below) is an error.
    fn read(&mut self) -> i32;
    
    /// Set pin to hight level
    fn set_high(&mut self);
    
    /// Set pin value to low level
    fn set_low(&mut self);
}

//#[cfg(feature = "use_rppal")]
struct GpioPinRppal {
    pin: u8,
    init_mode: rppal::gpio::Mode,
    rppal: rppal::gpio::Gpio,
}

//#[cfg(feature = "use_rppal")]
impl GpioPinRppal {
    fn new(pin: u32) -> Result<GpioPinRppal, Error> {
        let mut pp = rppal::gpio::Gpio::new().map_err(|e| {
            Error::new(ErrorKind::ConnectionAborted, e.description())
        })?;
        let mode = pp.mode(pin as u8).map_err(|e| {
            Error::new(ErrorKind::ConnectionAborted, e.description())
        })?;

        Ok(GpioPinRppal {
            rppal: pp,
            pin: pin as u8,
            init_mode: mode,
        })
    }
}

//#[cfg(feature = "use_rppal")]
impl GpioPin for GpioPinRppal {
    fn direction_input(&mut self) -> Result<(), Error> {
        self.rppal.set_mode(self.pin, rppal::gpio::Mode::Input);
        Ok(())
    }

    fn direction_output(&mut self, value: i32) -> Result<(), Error> {
        self.rppal.set_mode(self.pin, rppal::gpio::Mode::Output);
        self.set(value);
        Ok(())
    }

    fn set(&mut self, value: i32) {
        if value > 0 {
            self.rppal.write(self.pin, rppal::gpio::Level::High);
        } else {
            self.rppal.write(self.pin, rppal::gpio::Level::Low);
        }
    }

    fn set_high(&mut self) {
        self.set(1);
    }

    fn set_low(&mut self) {
        self.set(0);
    }

    fn read(&mut self) -> i32 {
        self.rppal.read(self.pin)
        .map( |it| match it {
            rppal::gpio::Level::High => 1,
            rppal::gpio::Level::Low => 0,
        }).unwrap_or(0)
    }
}


impl Drop for GpioPinRppal {
    fn drop(&mut self) {
        self.rppal.set_mode(self.pin, self.init_mode);
    }
}
