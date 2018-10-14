use gpio::{gpio_pin_new, GpioPin};

use std::error::Error;
use std::fmt;
use std::io::Error as IoError;
use std::io::ErrorKind as IoErrorKind;
use std::thread;
use std::time::{Duration, Instant};

/*
#[cfg(feature = "use_libc")]
use libc::{__errno_location, sched_get_priority_max, sched_getparam, sched_getscheduler,
           sched_param, sched_setscheduler, SCHED_FIFO};
*/

const MINIMUM_CACHE: u64 = 1250; // miliseconds
const CACHE_ON_ERROR: u64 = 5; //seconds

/// Determine DHT sensor types
#[derive(Debug, Clone)]
pub enum DhtType {
    DHT11,
    DHT21,
    DHT22,
}

/// Represent readings from DHT* sensor .
#[derive(Debug)]
pub struct DhtValue {
    dht_type: DhtType,
    value: [u8; 5],
}

impl DhtValue {
    /// Return temperature readings in Fahrenheit.
    pub fn temperature_f(&self) -> f32 {
        self.temperature() * 1.8 + 32.0
    }

    /// Return temperature readings in Celcius.
    pub fn temperature(&self) -> f32 {
        match &self.dht_type {
            DHT11 => self.value[2] as f32,
            _ => {
                let mut v: f32 = (self.value[2] & 0x7F) as f32;
                v = (v * 256.0 + self.value[3] as f32) * 0.1;
                if self.value[2] & 0x80 > 0 {
                    v *= -1.0;
                }
                v
            }
        }
    }

    /// Return humidity readins in percents.
    pub fn humidity(&self) -> f32 {
        match &self.dht_type {
            DHT11 => self.value[0] as f32,
            _ => {
                let mut v: f32 = self.value[0] as f32;
                v = (v * 256.0 + self.value[1] as f32) * 0.1;
                v
            }
        }
    }

    /// Return head index in Fahrenheit.
    /// Using both Rothfusz and Steadman's equations.
    pub fn heat_index_f(&self) -> f32 {
        heat_index(self.temperature_f(), self.humidity(), true)
    }

    /// Return head index in Celcius.
    /// Using both Rothfusz and Steadman's equations.
    pub fn heat_index_c(&self) -> f32 {
        heat_index(self.temperature(), self.humidity(), false)
    }
}

pub struct DhtSensor {
    pin: u8,
    dht_type: DhtType,
    gpio: Box<GpioPin>,
    last_read: Instant,
    value: [u8; 5],
}

/// Ideas about DHT reading sensors was found here:
/// - https://github.com/adafruit/DHT-sensor-library/blob/master/DHT.cpp
/// - https://github.com/adafruit/Adafruit_Python_DHT/blob/master/source/Raspberry_Pi/pi_dht_read.c
impl DhtSensor {
    pub fn new(pin: u8, dht_type: DhtType) -> Result<DhtSensor, Box<Error>> {
        let gpio = gpio_pin_new(pin as u32)?;
        DhtSensor::new_common(pin, dht_type, gpio)
    }

    fn new_common(
        pin: u8,
        dht_type: DhtType,
        mut gpio: Box<GpioPin>,
    ) -> Result<DhtSensor, Box<Error>> {
        gpio.direction_input();
        Ok(DhtSensor {
            pin: pin,
            dht_type: dht_type,
            gpio: gpio,
            last_read: Instant::now() - Duration::from_secs(1000),
            value: [0; 5],
        })
    }

    /// Try read sensor untill attempts limits will be reached.
    /// Repeat reading only on errorrs with little delay between reads.
    ///
    /// * `attempts` - Number of additional read attempts
    /// * `cache_sec` - Allow cached results acqured N seconds before
    pub fn read_until(&mut self, attempts: u8, cache_sec: u8) -> Result<DhtValue, IoError> {
        if Instant::now() - self.last_read < Duration::from_secs(cache_sec as u64) {
            return Ok(DhtValue {
                value: self.value,
                dht_type: self.dht_type.clone(),
            });
        }

        let mut res: Result<DhtValue, IoError> = Err(IoError::from(IoErrorKind::Other));
        let max_attempts = 1 + attempts;
        for i in 0..max_attempts {
            match self.read() {
                Ok(r) => {
                    return Ok(r);
                }
                Err(e) => {
                    // Sleep only on timout error
                    if e.kind() == IoErrorKind::TimedOut && i < (max_attempts - 1) {
                        thread::sleep(Duration::from_millis(150));
                    }
                    res = Err(e);
                }
            }
        }
        res
    }

    /// Read sensor in nice way.
    /// Will return recently cached value for frequently requests.
    /// On error can return value cached about 1 minute ago, to avoid unecessary errors in result.
    pub fn read(&mut self) -> Result<DhtValue, IoError> {
        // Check if cached value shoul be returned
        if Instant::now() - self.last_read < Duration::from_millis(MINIMUM_CACHE) {
            // To many erros if DHT reads to often
            return Ok(DhtValue {
                value: self.value,
                dht_type: self.dht_type.clone(),
            });
        }

        let raw_value = self.read_raw();
        if raw_value.is_ok() {
            return raw_value;
        }

        // Handle errors
        let cached_for = Instant::now() - self.last_read;
        return if cached_for <= Duration::from_secs(CACHE_ON_ERROR) {
            // Just return previously cached data assuming that temperature
            // delta for 2 secons is not huge
            Ok(DhtValue {
                value: self.value,
                dht_type: self.dht_type.clone(),
            })
        } else {
            raw_value
        };
    }

    /// Raw read from DHT sensor.
    /// Return result and data readed from sensor.
    /// Even on errors data can be not empty
    fn read_raw(&mut self) -> Result<DhtValue, IoError> {
        // Initialize variables
        let mut err: Option<IoError> = None;
        let mut data: [u8; 5] = [0; 5]; // Set 40 bits of received data to zero.
        let mut cycles: [u32; 83] = [0; 83];
        let read_limit = Instant::now() + Duration::from_millis(10);

        // Send start signal.  See DHT datasheet for full signal diagram:
        //   http://www.adafruit.com/datasheets/Digital%20humidity%20and%20temperature%20sensor%20AM2302.pdf
        // Go into high impedence state to let pull-up raise data line level and
        // start the reading process.
        self.gpio.direction_output(1);
        thread::sleep(Duration::from_millis(250));

        // Try to raise thread priority
        /*
        #[cfg(feature = "use_libc")]
        let mut prev_sched_param = sched_param { sched_priority: 0 };
        #[cfg(feature = "use_libc")]
        let prev_sched_policy = unsafe { sched_getscheduler(0) };
        #[cfg(feature = "use_libc")]
        {
            let priup = unsafe {
                sched_getparam(0, &mut prev_sched_param);
                sched_setscheduler(
                    0,
                    SCHED_FIFO,
                    &sched_param {
                        sched_priority: sched_get_priority_max(SCHED_FIFO),
                    },
                )
            };

            #[cfg(debug_assertions)]
            {
                if priup != 0 {
                    println!(
                        "DHT ERROR failed call to sched_setscheduler() with errno {}",
                        unsafe { *__errno_location() }
                    );
                }
            }
        }
        */
        // Time critical section begins
        {
            let end_sleep = Instant::now() + Duration::from_millis(20);
            // Voltage  level  from  high to  low.
            // This process must take at least 18ms to ensure DHT’s detection of MCU's signal.
            self.gpio.set_low();
            // Busy wait cycle should be better than //thread::sleep(Duration::from_millis(18));
            while Instant::now() < end_sleep {}

            self.gpio.direction_input();
            // MCU will pull up voltage and wait 20-40us for DHT’s response
            // Delay a bit to let sensor pull data line low.

            // READ to cycles[0] - or skip to next

            // Now start reading the data line to get the value from the DHT sensor.
            // First expect a low signal for ~80 microseconds followed by a high signal
            // for ~80 microseconds again.

            // READ to cycles[1] and cycles[2]

            // Now read the 40 bits sent by the sensor.  Each bit is sent as a 50
            // microsecond low pulse followed by a variable length high pulse.  If the
            // high pulse is ~28 microseconds then it's a 0 and if it's ~70 microseconds
            // then it's a 1.  We measure the cycle count of the initial 50us low pulse
            // and use that to compare to the cycle count of the high pulse to determine
            // if the bit is a 0 (high state cycle count < low state cycle count), or a
            // 1 (high state cycle count > low state cycle count). Note that for speed all
            // the pulses are read into a array and then examined in a later step.

            // READ to cycles[3+] as low level and cycles[4+] as high level

            let mut i = 0;
            let mut x = 0;
            while i < 83 {
                let v = self.gpio.read() == 1;
                if (i % 2 == 0) == v {
                    // Instead of reading time we just count number of cycles until next level value
                    cycles[i] += 1;
                } else {
                    i += 1;
                }

                // Check timeout
                x += 1;
                if x % 7000 == 0 && Instant::now() > read_limit {
                    err = Some(IoError::new(
                        IoErrorKind::TimedOut,
                        format!("Reading time exceeded 10ms"),
                    ));
                    break;
                }
            }
        }
        // Timing critical code is now complete.
        // Return priority to previous value
        /*
        #[cfg(feature = "use_libc")]
        unsafe { sched_setscheduler(0, prev_sched_policy, &prev_sched_param) };

        if err.is_some() {
            return Err(err.unwrap());
        }
        */
        //self.gpio.direction_output(1);

        // Inspect pulses and determine which ones are 0 (high state cycle count < low
        // state cycle count), or 1 (high state cycle count > low state cycle count).
        // We skip first 3 values because there is not data there
        for i in 0..40 {
            let low_cycle = cycles[2 * i + 3];
            let high_cycle = cycles[2 * i + 4];

            data[i / 8] <<= 1;
            if high_cycle > low_cycle {
                // High cycles are greater than 50us low cycle count, must be a 1.
                data[i / 8] |= 1;
            }
            // Else high cycles are less than (or equal to, a weird case) the 50us low
            // cycle count so this must be a zero.  Nothing needs to be changed in the
            // stored data.
        }

        #[cfg(feature = "debug_trace")]
        {
            print!("DHT readings: ");
            print!("{:X} {:X} {:X} {:X}", data[0], data[1], data[2], data[3]);
            println!(
                "  {:X} == {:X} (checksum)",
                data[4],
                (data[0] as u16 + data[1] as u16 + data[2] as u16 + data[3] as u16) & 0xFF
            );
        }

        // Check we read 40 bits and that the checksum matches.
        if data[4] as u16
            == ((data[0] as u16 + data[1] as u16 + data[2] as u16 + data[3] as u16) & 0xFF)
        {
            self.value = data;
            self.last_read = Instant::now();
            Ok(DhtValue {
                value: data,
                dht_type: self.dht_type.clone(),
            })
        } else {
            let err = IoError::new(IoErrorKind::InvalidData, format!("Checksum failure!",));
            Err(err)
        }
    }
}

impl fmt::Debug for DhtSensor {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "DHT ({:?} pin:{})", self.dht_type, self.pin)
    }
}

/// Calculate heat index Using both Rothfusz and Steadman's equations
/// http://www.wpc.ncep.noaa.gov/html/heatindex_equation.shtml
///
/// * `temp` - Temperature in Celsius of Fahrenheit
/// * `fahrenheit` - Define input and output temperature format (true for Fahrenheit)
fn heat_index(temp: f32, humidity: f32, fahrenheit: bool) -> f32 {
    let mut temperature = temp;
    if !fahrenheit {
        temperature = temp * 1.8 + 32.0;
    }
    let mut hi: f32 =
        0.5 * (temperature + 61.0 + ((temperature - 68.0) * 1.2) + (humidity * 0.094));

    if hi > 79.0 {
        hi = -42.379 + 2.04901523 * temperature + 10.14333127 * humidity
            + -0.22475541 * temperature * humidity
            + -0.00683783 * temperature.powf(2.0) + -0.05481717 * humidity.powf(2.0)
            + 0.00122874 * temperature.powf(2.0) * humidity
            + 0.00085282 * temperature * humidity.powf(2.0)
            + -0.00000199 * temperature.powf(2.0) * humidity.powf(2.0);

        if (humidity < 13.0) && (temperature >= 80.0) && (temperature <= 112.0) {
            hi -=
                ((13.0 - humidity) * 0.25) * ((17.0 - (temperature - 95.0).abs()) * 0.05882).sqrt();
        } else if (humidity > 85.0) && (temperature >= 80.0) && (temperature <= 87.0) {
            hi += ((humidity - 85.0) * 0.1) * ((87.0 - temperature) * 0.2);
        }
    }

    if fahrenheit {
        hi
    } else {
        (hi - 32.0) * 0.55555
    }
}
