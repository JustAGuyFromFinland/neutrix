use lazy_static::lazy_static;
use x86_64::structures::idt::*;
use core::arch::asm;

use crate::*;

lazy_static! {
    static ref IDT: InterruptDescriptorTable = {
        let mut idt = InterruptDescriptorTable::new();
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
		idt[32]
            .set_handler_fn(crate::pit::void);
		idt[33]
            .set_handler_fn(crate::kbd::keyboard_interrupt_handler);
        idt
    };
}

pub fn init_idt() {
    IDT.load();
}

pub fn hlt() -> ! {
    loop {
        x86_64::instructions::hlt();
    }
}