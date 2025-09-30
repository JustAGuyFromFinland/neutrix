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

	// Provide mapper / frame allocator pointers to drivers that map BARs
	// Safety: pass raw pointers to the global set functions used by drivers
	crate::driver_framework::drivers::vbe_vga::set_global_mapper_ptr(&mut mapper as *mut _);
	crate::driver_framework::drivers::vbe_vga::set_global_frame_allocator_ptr(&mut frame_allocator as *mut _);

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

	// Provide the global boot physical offset to drivers that need to map BARs
	crate::driver_framework::drivers::set_boot_phys_offset(phys_mem_offset.as_u64());

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

	// Register a logical console device and attach the console driver. This
	// lets other subsystems treat the console as a managed device and allows
	// future replacement with a windowing-backed console driver.
	let console_info = driver_framework::device::DeviceInfo {
		vendor_id: 0xfffe,
		device_id: 0xffff,
		class: 0xFF, // pseudo device class
		subclass: 0x00,
		prog_if: 0x00,
		resources: {
			let mut v = alloc::vec::Vec::new();
			v
		},
		capabilities: alloc::vec::Vec::new(),
		description: alloc::format!("Logical Console Device"),
	};

	let console_dev_id = crate::driver_framework::manager::GLOBAL_MANAGER.register_device(console_info);
	let console_drv = driver_framework::drivers::console::boxed_driver();
	if let Err(e) = crate::driver_framework::manager::GLOBAL_MANAGER.attach_driver(console_dev_id, console_drv) {
		println!("Failed to attach console driver: {}", e);
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

	// If CPU supports TSC and APIC is present, switch to TSC-deadline timer
	if hal::apic::is_initialized() && crate::arch::detect_cpu_features().tsc {
		// Disable PIT (already handled in enable_cpu_features if tsc was present),
		// initialize TSC-deadline timer and calibrate it against HPET if available
		if crate::arch::tsc_timer::init(&mut mapper, &mut frame_allocator, phys_mem_offset, 10) {
			println!("[TIMER] TSC-deadline timer initialized (calibrated if HPET present)");
		} else {
			println!("[TIMER] TSC-deadline timer not enabled (missing features or calibration failed)");
		}
	}
	x86_64::instructions::interrupts::enable();

	// Print registered devices for debugging (human-readable class/subclass)
	crate::driver_framework::manager::GLOBAL_MANAGER.list_devices();

	// Attach VBE/linear framebuffer driver to any discovered PCI display controller
	// (class 0x03). Do not hold GLOBAL_MANAGER.devices lock while calling attach_driver
	// (it will re-lock internally).
	let mut match_ids: alloc::vec::Vec<usize> = alloc::vec::Vec::new();
	{
		let devices = crate::driver_framework::manager::GLOBAL_MANAGER.devices.lock();
		for entry in devices.iter() {
			let info = entry.device.info();
			if info.class == 0x03 {
				match_ids.push(entry.device.id());
			}
		}
	}

	for dev_id in match_ids.into_iter() {
		// Try to attach our VBE driver for any display controller found
		let drv = driver_framework::drivers::vbe_vga::boxed_driver();
		// Ignore attach errors (probe/start may fail on some hardware)
		let _ = crate::driver_framework::manager::GLOBAL_MANAGER.attach_driver(dev_id, drv);
	}

	// If VBE driver activated, clear screen and print a short message
	cls!();
	println!("neutrix: vbe framebuffer ready\n");

	// Manually register a PS/2 mouse device (legacy IRQ-based)
	let mouse_info = driver_framework::device::DeviceInfo {
		vendor_id: 0xffff,
		device_id: 0xffff,
		class: 0x09,
		subclass: 0x00,
		prog_if: 0x00,
		resources: {
			let mut v = alloc::vec::Vec::new();
			v.push(driver_framework::device::Resource { kind: driver_framework::device::ResourceKind::Interrupt(44), addr: 0, len: 0 });
			v
		},
		capabilities: alloc::vec::Vec::new(),
		description: alloc::format!("PS/2 Mouse"),
	};

	let mouse_dev_id = crate::driver_framework::manager::GLOBAL_MANAGER.register_device(mouse_info);
	let mouse_drv = driver_framework::drivers::ps2mouse::boxed_driver();
	if let Err(e) = crate::driver_framework::manager::GLOBAL_MANAGER.attach_driver(mouse_dev_id, mouse_drv) {
		println!("Failed to attach PS/2 mouse driver: {}", e);
	} else {
		// If we have framebuffer info, set cursor to center
		if let Some(info) = crate::driver_framework::drivers::vbe_vga::get_fb_info() {
			let cx = (info.width as i32) / 2;
			let cy = (info.height as i32) / 2;
			crate::driver_framework::drivers::ps2mouse::set_cursor_pos(cx, cy);
		}

		// Ensure IOAPIC redirection entry for the PS/2 device is unmasked.
		// Prefer to map using ACPI ISOs if present so we unmask the correct GSI.
		if hal::apic::is_initialized() {
			if let Some(apic_id) = hal::apic::local_apic_id() {
				// Find the interrupt vector resource on the device (we registered one earlier)
				let devinfo_opt = {
					let devices = crate::driver_framework::manager::GLOBAL_MANAGER.devices.lock();
					devices.iter().find(|e| e.device.id == mouse_dev_id).map(|e| e.device.info())
				};
				// If device info isn't available, fall back to legacy IRQ 12
				if let Some(devinfo) = devinfo_opt {
					let mut handled = false;
					for r in devinfo.resources.iter() {
						if let driver_framework::device::ResourceKind::Interrupt(vec) = r.kind {
							let vector = vec;
							// Legacy IRQ candidate = vector - 0x20
							let legacy_irq = (vector as u32).wrapping_sub(0x20u32) & 0xFF;
							// Try to find an ISO that maps this legacy IRQ to a GSI
							let mut gsi_candidate = legacy_irq; // fallback
							let isos = crate::devices::acpi::get_isos();
							for iso in isos.iter() {
								if iso.source as u32 == legacy_irq {
									gsi_candidate = iso.gsi;
									break;
								}
							}

							if hal::ioapic::unmask_gsi(gsi_candidate, vector, apic_id, phys_mem_offset) {
								println!("[MAIN] Unmasked IOAPIC GSI {} -> vector 0x{:x} apic {}", gsi_candidate, vector, apic_id);
								if let Some((low, high)) = hal::ioapic::read_redirection_entry(gsi_candidate, phys_mem_offset) {
									println!("[MAIN] IOAPIC GSI {} redir low=0x{:08x} high=0x{:08x}", gsi_candidate, low, high);
								}
							} else {
								println!("[MAIN] Failed to unmask IOAPIC GSI {} (vector 0x{:x})", gsi_candidate, vector);
							}
							handled = true;
						}
					}
					if !handled {
						// no interrupt resource found; try legacy IRQ 12 as last resort
						let legacy_irq = 12u32;
						let vector = 0x20u8.wrapping_add(12u8);
						if hal::ioapic::unmask_gsi(legacy_irq, vector, apic_id, phys_mem_offset) {
							println!("[MAIN] Unmasked IOAPIC fallback GSI {} -> vector 0x{:x} apic {}", legacy_irq, vector, apic_id);
							if let Some((low, high)) = hal::ioapic::read_redirection_entry(legacy_irq, phys_mem_offset) {
								println!("[MAIN] IOAPIC GSI {} redir low=0x{:08x} high=0x{:08x}", legacy_irq, low, high);
							}
						} else {
							println!("[MAIN] Failed to unmask IOAPIC fallback GSI {}", legacy_irq);
						}
					}
				} else {
					// Could not retrieve device info, fallback
					let legacy_irq = 12u32;
					let vector = 0x20u8.wrapping_add(12u8);
					if hal::ioapic::unmask_gsi(legacy_irq, vector, apic_id, phys_mem_offset) {
						println!("[MAIN] Unmasked IOAPIC fallback GSI {} -> vector 0x{:x} apic {}", legacy_irq, vector, apic_id);
						if let Some((low, high)) = hal::ioapic::read_redirection_entry(legacy_irq, phys_mem_offset) {
							println!("[MAIN] IOAPIC GSI {} redir low=0x{:08x} high=0x{:08x}", legacy_irq, low, high);
						}
					} else {
						println!("[MAIN] Failed to unmask IOAPIC fallback GSI {}", legacy_irq);
					}
				}
			} else {
				println!("[MAIN] APIC initialized but failed to read local APIC id for IOAPIC unmask");
			}
		}

		// Spawn a background task to process mouse packets outside interrupt context
		
	}

	let mut executor = Executor::new();
	executor.spawn(Task::new(driver_framework::drivers::ps2kbd::print_keypresses()));
	executor.spawn(Task::new(driver_framework::drivers::ps2mouse::mouse_event_loop()));
	executor.run();
	hlt();
}

/// This function is called on panic.
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    println!("{}", info);
    hlt();
}