#![no_std]   // likely for your kernel, removes std
#![no_main]  // if you boot directly without an OS
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![allow(warnings)]

// Provide the `alloc` crate to modules that need heap types (Vec, Box, String).
extern crate alloc;

pub mod arch;
pub use arch::*;
pub mod bootvga;
pub use bootvga::*;
pub mod rlib;
pub use rlib::*;
pub mod devices;
pub use devices::*;
pub mod memory;
pub use memory::*;
pub mod hal;
pub use hal::*;
pub mod driver_framework;
pub use driver_framework::*;
