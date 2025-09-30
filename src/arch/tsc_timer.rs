use crate::*;
use core::arch::asm;
use core::sync::atomic::{AtomicU64, Ordering};
use x86_64::structures::idt::InterruptStackFrame;
use x86_64::VirtAddr;
use crate::devices::acpi;
use x86_64::structures::paging::Translate;
use x86_64::structures::paging::Mapper;
use x86_64::PhysAddr;
use x86_64::structures::paging::{OffsetPageTable, Page, PhysFrame, Size4KiB, FrameAllocator, PageTableFlags as Flags};

const IA32_TSC_DEADLINE: u32 = 0x6E0;

static PERIOD_CYCLES: AtomicU64 = AtomicU64::new(10_000_000); // default: 10M cycles (~10ms @1GHz)
const HPET_MAIN_COUNTER_OFFSET: u64 = 0xF0;

pub fn rdtsc() -> u64 {
    unsafe {
        let low: u32;
        let high: u32;
        asm!("rdtsc", out("eax") low, out("edx") high);
        ((high as u64) << 32) | (low as u64)
    }
}

unsafe fn write_msr(msr: u32, val: u64) {
    let low = val as u32;
    let high = (val >> 32) as u32;
    asm!("wrmsr", in("ecx") msr, in("eax") low, in("edx") high);
}

/// Timer IRQ handler used when TSC-deadline is enabled.
/// It re-arms the deadline and issues EOI.
pub extern "x86-interrupt" fn tsc_timer_handler(_stack_frame: InterruptStackFrame) {
    // compute next deadline and program MSR
    let period = PERIOD_CYCLES.load(Ordering::SeqCst);
    let now = rdtsc();
    let next = now.wrapping_add(period);
    unsafe { write_msr(IA32_TSC_DEADLINE, next); }

    unsafe {
        if crate::hal::apic::is_initialized() {
            crate::hal::apic::send_eoi();
        } else {
            crate::arch::interrupts::PICS.lock().notify_end_of_interrupt(InterruptIndex::Timer.as_u8());
        }
    }
}

/// Initialize TSC-deadline timer.
/// If HPET is available (via ACPI) the function will calibrate the TSC frequency
/// against the HPET main counter and set the period to desired_ms milliseconds.
/// `phys_offset` is the kernel's physical memory offset used to map device MMIO.
pub fn init(mapper: &mut OffsetPageTable<'static>, frame_allocator: &mut impl FrameAllocator<Size4KiB>, phys_offset: VirtAddr, desired_ms: u64) -> bool {
    // Ensure CPU supports MSR and TSC-deadline before attempting to program MSR
    let feats = crate::arch::detect_cpu_features();
    if !feats.msr || !feats.tsc_deadline || !feats.tsc {
        // CPU doesn't support required features; do not enable TSC-deadline
        return false;
    }

    // Try HPET-based calibration if available
    if let Some(hpet_base) = acpi::get_hpet_address() {
        // Avoid mapping very high MMIO regions here — if HPET is located
        // above 4GiB and the kernel hasn't mapped that region into its
        // direct-physical mapping, reads will cause a page fault. For
        // safety, only attempt calibration for HPET bases in low physical
        // memory. This is conservative; we can relax it later with a
        // proper mapper-based mapping.
        if hpet_base < 0x1_0000_0000u64 {
            let main_phys = hpet_base + HPET_MAIN_COUNTER_OFFSET;

            // Map HPET main counter pages into the kernel address space (page-aligned)
            let page_start = main_phys & !0xFFFu64;
            let offset_in_page = (main_phys & 0xFFF) as usize;
            let needed = ((offset_in_page + core::mem::size_of::<u64>()) + 0xFFF) / 0x1000;

            for i in 0..needed {
                let phys_page = page_start + (i as u64) * 0x1000u64;
                let virt_page_addr = phys_offset.as_u64().wrapping_add(phys_page);
                let page: Page<Size4KiB> = Page::containing_address(VirtAddr::new(virt_page_addr));

                // If already mapped, skip
                if mapper.translate_addr(VirtAddr::new(virt_page_addr)).is_some() {
                    continue;
                }

                let frame = PhysFrame::containing_address(PhysAddr::new(phys_page));
                let flags = Flags::PRESENT | Flags::WRITABLE;
                let map_to_result = unsafe { mapper.map_to(page, frame, flags, frame_allocator) };
                match map_to_result {
                    Ok(flush) => { flush.flush(); }
                    Err(_) => { /* mapping failed; skip calibration */ return false; }
                }
            }

            let main_virt = (main_phys + phys_offset.as_u64()) as *const u64;

            // read period in femtoseconds
            let period_fs = acpi::get_hpet_period_fs();
            if period_fs != 0 {
                // sample HPET and TSC over a short interval
                unsafe {
                    let h1 = core::ptr::read_volatile(main_virt);
                    let t1 = rdtsc();
                    // wait until HPET advances by at least 1000 ticks (should be fast)
                    while core::ptr::read_volatile(main_virt).wrapping_sub(h1) < 1000 {
                        core::arch::asm!("nop");
                    }
                    let h2 = core::ptr::read_volatile(main_virt);
                    let t2 = rdtsc();
                    let hdelta = h2.wrapping_sub(h1) as u128;
                    let tdelta = t2.wrapping_sub(t1) as u128;

                    if hdelta != 0 {
                        // HPET period is in femtoseconds -> 1e15 femtoseconds = 1 second
                        // We'll compute tsc_hz = (tdelta * 1e15) / (hdelta * period_fs)
                        let num = tdelta.saturating_mul(1_000_000_000_000_000u128);
                        let den = hdelta.saturating_mul(period_fs as u128);
                        if den != 0 {
                            let tsc_hz = num / den;
                            // desired cycles for desired_ms milliseconds
                            let cycles = (tsc_hz * (desired_ms as u128)) / 1000u128;
                            if cycles > 0 {
                                PERIOD_CYCLES.store(cycles as u64, Ordering::SeqCst);
                            }
                        }
                    }
                }
            }
        }
    }

    // Register handler for timer vector and arm initial deadline
    let vec = crate::arch::interrupts::InterruptIndex::Timer.as_u8();
    crate::arch::idt::register_irq_handler(vec, tsc_timer_handler);

    let period = PERIOD_CYCLES.load(Ordering::SeqCst);
    let now = rdtsc();
    let next = now.wrapping_add(period);
    unsafe { write_msr(IA32_TSC_DEADLINE, next); }
    true
}
