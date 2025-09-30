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
use x86_64::instructions::port::Port;

entry_point!(kernel_main);

extern crate neutrix;
extern crate alloc;

use neutrix::*;
use crate::driver_framework::drivers::ps2kbd;

fn kernel_main(boot_info: &'static BootInfo) -> ! {
	enable_sse();
	let phys_mem_offset = VirtAddr::new(boot_info.physical_memory_offset);
	
	// Initialize paging and frame allocator first so we can set up the heap
	let mut mapper = unsafe { memory::init(phys_mem_offset) };
	// Try to obtain a linker-provided kernel end symbol (optional).
	// If it's not present (e.g., building with LLVM on Windows), fall back to
	// a conservative heuristic: reserve the page containing `kernel_main` and
	// the next 1 MiB to avoid allocating kernel code/data pages.
	// Compute a conservative kernel_reserved_end using the address of `kernel_main`.
	// This avoids requiring a linker-provided symbol on platforms where it's not available.
	let kernel_reserved_end = unsafe {
		// Fallback: take address of `kernel_main` and reserve a 1 MiB window
		let fn_virt = kernel_main as usize as u64;
		let phys_offset_val = phys_mem_offset.as_u64();
		if fn_virt <= phys_offset_val {
			None
		} else {
			let fn_phys = fn_virt - phys_offset_val;
			let page_aligned = fn_phys & !0xFFFu64;
			Some(page_aligned + 1024 * 1024) // reserve 1 MiB after kernel page
		}
	};

	let mut frame_allocator = unsafe {
		BootInfoFrameAllocator::init(&boot_info.memory_map, phys_mem_offset, kernel_reserved_end)
	};

	// Initialize the global heap before calling HAL so modules that use
	// `alloc` (Vec/Box) during ACPI/MADT parsing have a working allocator.
	allocator::init_heap(&mut mapper, &mut frame_allocator)
		.expect("heap initialization failed");

	init_gdt();
	setcolor!(Color::Yellow, Color::Black);
	init_idt();

	// Initialize hardware through HAL (ACPI parsing may allocate)
	let (cpu_info, acpi_status) = hal::init_hardware(phys_mem_offset);

	// Scan PCI devices and register them with the device manager (no drivers attached yet)
	// Pass the physical memory offset so PCI code can probe MMIO (MSI-X tables)
	devices::pci::scan_and_register_with_phys_offset(phys_mem_offset.as_u64());

	// Manually register a PS/2 keyboard device (not discoverable via PCI)
	let kbd_info = driver_framework::device::DeviceInfo {
 		vendor_id: 0xffff,
 		device_id: 0xffff,
 		class: 0x09, // Input Device
 		subclass: 0x00,
 		prog_if: 0x00,
 		resources: {
 			let mut v = alloc::vec::Vec::new();
 			v.push(driver_framework::device::Resource { kind: driver_framework::device::ResourceKind::Interrupt(33), addr: 0, len: 0 });
 			v
 		},
 		capabilities: alloc::vec::Vec::new(),
 		description: alloc::format!("PS/2 Keyboard"),
 	};

	let dev_id = crate::driver_framework::manager::GLOBAL_MANAGER.register_device(kbd_info);
	// Attach our KMDF-style ps2 keyboard driver
	let drv = driver_framework::drivers::ps2kbd::boxed_driver();
	if let Err(e) = crate::driver_framework::manager::GLOBAL_MANAGER.attach_driver(dev_id, drv) {
 		println!("Failed to attach PS/2 keyboard driver: {}", e);
 	}

	// Continue with architecture-specific initialization
	// Do not initialize legacy PICs when running with APIC-only interrupts.
	// Instead, mask (disable) both PICs so they don't deliver IRQs.
	unsafe {
		// Mask all IRQs on the legacy PICs by writing 0xFF to their data ports
		// Master PIC data port = 0x21, Slave PIC data port = 0xA1
		let mut master_data: Port<u8> = Port::new(0x21);
		let mut slave_data: Port<u8> = Port::new(0xA1);
		// Safety: writing to I/O ports is inherently unsafe; this masks all PIC IRQs
		master_data.write(0xFFu8);
		slave_data.write(0xFFu8);
	}
	// Program IOAPIC ISOs to target this CPU and unmask them now that IDT is ready.
	if hal::apic::is_initialized() {
		if let Some(apic_id) = hal::apic::local_apic_id() {
			hal::ioapic::enable_isos_for_local(phys_mem_offset, apic_id);
		} else {
			println!("[HAL] APIC initialized but failed to read local APIC id");
		}
	}
	x86_64::instructions::interrupts::enable();

	// Print registered devices for debugging (human-readable class/subclass)
	crate::driver_framework::manager::GLOBAL_MANAGER.list_devices();

	let mut executor = Executor::new();
	executor.spawn(Task::new(driver_framework::drivers::ps2kbd::print_keypresses()));
	executor.run();
	hlt();
}

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    hlt();
}