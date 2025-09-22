use crate::*;
use x86_64::structures::idt::*;

pub extern "x86-interrupt" fn void(
    _stack_frame: InterruptStackFrame)
{
    unsafe {
        // If Local APIC is present use APIC EOI, otherwise notify PICs
        if crate::hal::apic::is_initialized() {
            crate::hal::apic::send_eoi();
        } else {
            PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
}