#![no_std]   // likely for your kernel, removes std
#![no_main]  // if you boot directly without an OS
#![feature(abi_x86_interrupt)]
#![feature(alloc_error_handler)]
#![allow(warnings)]
#![feature(alloc)]

pub extern crate alloc;
pub use alloc::*;
pub use alloc::alloc::*;
pub use alloc::boxed::*;

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

