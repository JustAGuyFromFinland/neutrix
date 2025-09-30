use lazy_static::lazy_static;
use x86_64::structures::idt::*;
use core::arch::asm;

use crate::*;
use alloc::boxed::Box;
use core::sync::atomic::{AtomicPtr, Ordering};

use x86_64::structures::idt::InterruptStackFrame;

// Atomic pointer to the leaked IDT. We initialize on first use in a thread-safe
// manner. AtomicPtr is Sync and safe to use in statics.
static IDT_PTR: AtomicPtr<InterruptDescriptorTable> = AtomicPtr::new(core::ptr::null_mut());

/// Ensure the IDT is created and leaked; return the raw pointer.
fn ensure_idt_initialized() -> *mut InterruptDescriptorTable {
	let mut ptr = IDT_PTR.load(Ordering::SeqCst);
	if ptr.is_null() {
		// Allocate and initialize the IDT
		let mut idt = InterruptDescriptorTable::new();

		// Exceptions / traps
		idt.divide_error.set_handler_fn(division_by_zero);
		idt.debug.set_handler_fn(debug_error);
		idt.overflow.set_handler_fn(overflow);
		idt.bound_range_exceeded.set_handler_fn(bre);
		idt.invalid_opcode.set_handler_fn(invalid_opcode);
		idt.device_not_available.set_handler_fn(dno);
		idt.breakpoint.set_handler_fn(breakpoint);
		unsafe {
			idt.double_fault.set_handler_fn(double_fault)
				.set_stack_index(gdt::DOUBLE_FAULT_IST_INDEX);
		}
		idt.invalid_tss.set_handler_fn(invalid_tss);
		idt.segment_not_present.set_handler_fn(snp);
		idt.stack_segment_fault.set_handler_fn(ssf);
		idt.general_protection_fault.set_handler_fn(gpf);
		idt.page_fault.set_handler_fn(pf);

		// Default IRQ handlers
		for vec in 32u8..=255u8 {
			unsafe { idt[vec].set_handler_fn(default_irq_handler); }
		}

		let leaked = Box::leak(Box::new(idt)) as *mut InterruptDescriptorTable;
		// Attempt to publish the pointer atomically; if another thread raced and
		// set it first, drop our leaked box by ignoring (we can't free leaked),
		// but prefer the published pointer.
		let prev = IDT_PTR.compare_and_swap(core::ptr::null_mut(), leaked, Ordering::SeqCst);
		if prev.is_null() {
			ptr = leaked;
		} else {
			ptr = prev; // use the already-published one
		}
	}
	ptr
}

/// The signature drivers must use when registering an IRQ handler.
pub type IrqHandler = extern "x86-interrupt" fn(InterruptStackFrame);

/// Default IRQ handler used until a driver registers a real one. It simply
/// prints a message and issues an EOI so the interrupt line is cleared.
pub extern "x86-interrupt" fn default_irq_handler(_stack_frame: InterruptStackFrame) {
	println!("[INT] received unhandled IRQ (placeholder)");
	unsafe {
		if crate::hal::apic::is_initialized() {
			crate::hal::apic::send_eoi();
		} else {
			// If using PICs this should be the IRQ number; we don't know it here
			// so call notify_end_of_interrupt with 0 as a safe no-op in many
			// implementations — however we prefer APIC mode as per main.rs.
			crate::arch::interrupts::PICS.lock().notify_end_of_interrupt(0);
		}
	}
}

/// Register an IRQ handler for `vector`. The handler must use the
/// `extern "x86-interrupt" fn(InterruptStackFrame)` ABI (i.e. no error code).
/// Replaces any existing handler for that vector. Calling this for exception
/// vectors that push an error code is invalid.
pub fn register_irq_handler(vector: u8, handler: IrqHandler) {
	let ptr = ensure_idt_initialized();
	unsafe { (&mut *ptr)[vector].set_handler_fn(handler); }
}

/// Unregister the handler for `vector` and restore the default placeholder.
pub fn unregister_irq_handler(vector: u8) {
	let ptr = ensure_idt_initialized();
	unsafe { (&mut *ptr)[vector].set_handler_fn(default_irq_handler); }
}

pub fn init_idt() {
	// Load the (possibly modified) IDT. `load` requires a `'static` reference
	// so obtain one from the leaked pointer.
	let ptr = ensure_idt_initialized();
	let idt_ref: &'static InterruptDescriptorTable = unsafe { &*ptr };
	idt_ref.load();
}

pub fn hlt() -> ! {
	loop { x86_64::instructions::hlt(); }
}