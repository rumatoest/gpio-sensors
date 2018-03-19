#![crate_type = "lib"]
#![crate_name = "gpio_sensors"]

#[cfg(feature = "use_libc")]
extern crate libc;
//#[cfg(feature = "use_rppal")]
extern crate rppal;

pub mod gpio;
pub mod dht;

#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
