use x86_64::registers::control::{Cr0, Cr4};
use x86_64::registers::control::Cr0Flags;
use x86_64::registers::control::Cr4Flags;

pub fn enable_sse() {
    unsafe {
        // Enable FPU/SSE instructions
        let mut cr0 = Cr0::read();
        cr0.remove(Cr0Flags::EMULATE_COPROCESSOR); // clear EM = 0
        cr0.insert(Cr0Flags::MONITOR_COPROCESSOR); // set MP = 1
        Cr0::write(cr0);

        let mut cr4 = Cr4::read();
        cr4.insert(Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE);
        Cr4::write(cr4);
    }
}