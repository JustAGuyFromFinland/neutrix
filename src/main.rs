#![no_std]
#![no_main]
#![feature(asm)]
#![feature(abi_x86_interrupt)]
#![allow(warnings)]
#![feature(alloc)]

use core::arch::asm;
use core::panic::PanicInfo;
use bootloader::*;
use x86_64::{structures::paging::Translate, VirtAddr, structures::paging::Page};

entry_point!(kernel_main);

extern crate neutrix;

use neutrix::*;

fn kernel_main(boot_info: &'static BootInfo) -> ! {
	enable_sse();
	init_gdt();
	setcolor!(Color::Yellow, Color::Blue);
	cls!();
	init_idt();
	unsafe {interrupts::PICS.lock().initialize()};
	x86_64::instructions::interrupts::enable();
	let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
    let mut mapper = unsafe { memory::init(phys_mem_offset) };
    let mut frame_allocator = unsafe {
        BootInfoFrameAllocator::init(&boot_info.memory_map)
    };
	allocator::init_heap(&mut mapper, &mut frame_allocator)
        .expect("heap initialization failed");
	let mut executor = Executor::new();
	executor.spawn(Task::new(print_keypresses()));
    executor.run();
	hlt();
}

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    hlt();
}