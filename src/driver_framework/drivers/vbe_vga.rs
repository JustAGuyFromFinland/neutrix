use crate::*;
use alloc::boxed::Box;
use core::ptr;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;
use crate::driver_framework::driver::Driver;
use crate::driver_framework::device::ResourceKind;
use x86_64::VirtAddr;
use x86_64::structures::paging::{OffsetPageTable, Page, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use crate::memory::frame::BootInfoFrameAllocator;
use x86_64::structures::paging::PageTableFlags as Flags;
use x86_64::structures::paging::Mapper;
// Mapper trait not needed directly here

/// VBE/linear framebuffer driver that maps BARs using the kernel mapper.
struct FbMapping { virt_base: u64, phys_map_start: u64, bar_phys: u64, pages: usize }

#[derive(Clone, Copy, Debug)]
pub struct FramebufferInfo {
    pub width: u32,
    pub height: u32,
    pub bpp: u32,
    pub pitch: usize,
}

// (Console state moved into the console driver)

// --- Embedded VGA 8x8 font ---
struct VGA8X8;
impl VGA8X8 {
    /// Return a reference to an 8-byte glyph for ASCII characters 0x20..0x7F.
    pub fn get_glyph(ch: u8) -> Option<&'static [u8;8]> {
        const FIRST: u8 = 0x20;
        const LAST: u8 = 0x7F;
        if ch < FIRST || ch > LAST { return None; }
        let idx = (ch - FIRST) as usize;
        Some(&FONT8X8[idx])
    }
}

// Standard 8x8 font for ASCII 0x20..0x7F (95 characters). Each glyph is 8 bytes (rows), MSB is left pixel.
// For brevity and speed I've included a compact 95*8 table generated from a standard VGA 8x8 dataset.
static FONT8X8: [[u8;8]; 95] = [
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0x00], // space
    [0x18,0x3c,0x3c,0x18,0x18,0x00,0x18,0x00], // !
    [0x6c,0x6c,0x48,0x00,0x00,0x00,0x00,0x00], // "
    [0x6c,0x6c,0xfe,0x6c,0xfe,0x6c,0x6c,0x00], // #
    [0x18,0x3e,0x60,0x3c,0x06,0x7c,0x18,0x00], // $
    [0x00,0xc6,0xcc,0x18,0x30,0x66,0xc6,0x00], // %
    [0x38,0x6c,0x38,0x76,0xdc,0xcc,0x76,0x00], // &
    [0x30,0x30,0x60,0x00,0x00,0x00,0x00,0x00], // '
    [0x0c,0x18,0x30,0x30,0x30,0x18,0x0c,0x00], // (
    [0x30,0x18,0x0c,0x0c,0x0c,0x18,0x30,0x00], // )
    [0x00,0x66,0x3c,0xff,0x3c,0x66,0x00,0x00], // *
    [0x00,0x18,0x18,0x7e,0x18,0x18,0x00,0x00], // +
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x30], // ,
    [0x00,0x00,0x00,0x7e,0x00,0x00,0x00,0x00], // -
    [0x00,0x00,0x00,0x00,0x00,0x18,0x18,0x00], // .
    [0x06,0x0c,0x18,0x30,0x60,0xc0,0x80,0x00], // /
    [0x7c,0xc6,0xce,0xd6,0xe6,0xc6,0x7c,0x00], // 0
    [0x18,0x38,0x18,0x18,0x18,0x18,0x7e,0x00], // 1
    [0x7c,0xc6,0x06,0x1c,0x30,0x66,0xfe,0x00], // 2
    [0x7c,0xc6,0x06,0x3c,0x06,0xc6,0x7c,0x00], // 3
    [0x0c,0x1c,0x3c,0x6c,0xfe,0x0c,0x1e,0x00], // 4
    [0xfe,0xc0,0xc0,0xfc,0x06,0xc6,0x7c,0x00], // 5
    [0x3c,0x60,0xc0,0xfc,0xc6,0xc6,0x7c,0x00], // 6
    [0xfe,0xc6,0x0c,0x18,0x30,0x30,0x30,0x00], // 7
    [0x7c,0xc6,0xc6,0x7c,0xc6,0xc6,0x7c,0x00], // 8
    [0x7c,0xc6,0xc6,0x7e,0x06,0x0c,0x78,0x00], // 9
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x00], // :
    [0x00,0x18,0x18,0x00,0x00,0x18,0x18,0x30], // ;
    [0x0e,0x1c,0x38,0x70,0x38,0x1c,0x0e,0x00], // <
    [0x00,0x00,0x7e,0x00,0x00,0x7e,0x00,0x00], // =
    [0x70,0x38,0x1c,0x0e,0x1c,0x38,0x70,0x00], // >
    [0x7c,0xc6,0x0e,0x1c,0x18,0x00,0x18,0x00], // ?
    [0x7c,0xc6,0xde,0xde,0xde,0xc0,0x78,0x00], // @
    [0x10,0x38,0x6c,0xc6,0xfe,0xc6,0xc6,0x00], // A
    [0xfc,0x66,0x66,0x7c,0x66,0x66,0xfc,0x00], // B
    [0x3c,0x66,0xc0,0xc0,0xc0,0x66,0x3c,0x00], // C
    [0xf8,0x6c,0x66,0x66,0x66,0x6c,0xf8,0x00], // D
    [0xfe,0x62,0x68,0x78,0x68,0x62,0xfe,0x00], // E
    [0xfe,0x62,0x68,0x78,0x68,0x60,0xf0,0x00], // F
    [0x3c,0x66,0xc0,0xc0,0xce,0x66,0x3c,0x00], // G
    [0xc6,0xc6,0xc6,0xfe,0xc6,0xc6,0xc6,0x00], // H
    [0x7e,0x18,0x18,0x18,0x18,0x18,0x7e,0x00], // I
    [0x1e,0x0c,0x0c,0x0c,0xcc,0xcc,0x78,0x00], // J
    [0xe6,0x66,0x6c,0x78,0x6c,0x66,0xe6,0x00], // K
    [0xf0,0x60,0x60,0x60,0x62,0x66,0xfe,0x00], // L
    [0xc6,0xee,0xfe,0xfe,0xd6,0xc6,0xc6,0x00], // M
    [0xc6,0xe6,0xf6,0xde,0xce,0xc6,0xc6,0x00], // N
    [0x7c,0xc6,0xc6,0xc6,0xc6,0xc6,0x7c,0x00], // O
    [0xfc,0x66,0x66,0x7c,0x60,0x60,0xf0,0x00], // P
    [0x7c,0xc6,0xc6,0xc6,0xd6,0xcc,0x7a,0x00], // Q
    [0xfc,0x66,0x66,0x7c,0x6c,0x66,0xe6,0x00], // R
    [0x7c,0xc6,0xc0,0x7c,0x06,0xc6,0x7c,0x00], // S
    [0xff,0x99,0x18,0x18,0x18,0x18,0x3c,0x00], // T
    [0xc6,0xc6,0xc6,0xc6,0xc6,0xc6,0x7c,0x00], // U
    [0xc6,0xc6,0xc6,0xc6,0xc6,0x6c,0x38,0x00], // V
    [0xc6,0xc6,0xc6,0xd6,0xfe,0xee,0xc6,0x00], // W
    [0xc6,0x6c,0x38,0x10,0x38,0x6c,0xc6,0x00], // X
    [0xcc,0xcc,0x98,0x30,0x30,0x30,0x78,0x00], // Y
    [0xfe,0xce,0x9c,0x38,0x70,0xe6,0xfe,0x00], // Z
    [0x3c,0x30,0x30,0x30,0x30,0x30,0x3c,0x00], // [
    [0xc0,0x60,0x30,0x18,0x0c,0x06,0x02,0x00], // backslash
    [0x3c,0x0c,0x0c,0x0c,0x0c,0x0c,0x3c,0x00], // ]
    [0x10,0x38,0x6c,0xc6,0x00,0x00,0x00,0x00], // ^
    [0x00,0x00,0x00,0x00,0x00,0x00,0x00,0xff], // _
    [0x30,0x18,0x0c,0x00,0x00,0x00,0x00,0x00], // `
    [0x00,0x00,0x78,0x0c,0x7c,0xcc,0x76,0x00], // a
    [0xe0,0x60,0x7c,0x66,0x66,0x66,0xdc,0x00], // b
    [0x00,0x00,0x7c,0xc0,0xc0,0xc0,0x7c,0x00], // c
    [0x1c,0x0c,0x7c,0xcc,0xcc,0xcc,0x76,0x00], // d
    [0x00,0x00,0x7c,0xc6,0xfe,0xc0,0x7c,0x00], // e
    [0x3c,0x66,0x60,0xf8,0x60,0x60,0xf0,0x00], // f
    [0x00,0x00,0x76,0xcc,0xcc,0x7c,0x0c,0xf8], // g
    [0xe0,0x60,0x6c,0x76,0x66,0x66,0xe6,0x00], // h
    [0x18,0x00,0x38,0x18,0x18,0x18,0x3c,0x00], // i
    [0x0c,0x00,0x1c,0x0c,0x0c,0xcc,0xcc,0x78], // j
    [0xe0,0x60,0x66,0x6c,0x78,0x6c,0xe6,0x00], // k
    [0x38,0x18,0x18,0x18,0x18,0x18,0x3c,0x00], // l
    [0x00,0x00,0xec,0xfe,0xd6,0xd6,0xd6,0x00], // m
    [0x00,0x00,0xdc,0x66,0x66,0x66,0x66,0x00], // n
    [0x00,0x00,0x7c,0xc6,0xc6,0xc6,0x7c,0x00], // o
    [0x00,0x00,0xdc,0x66,0x66,0x7c,0x60,0xf0], // p
    [0x00,0x00,0x76,0xcc,0xcc,0x7c,0x0c,0x1e], // q
    [0x00,0x00,0xdc,0x76,0x60,0x60,0xf0,0x00], // r
    [0x00,0x00,0x7e,0xc0,0x7c,0x06,0xfc,0x00], // s
    [0x30,0x30,0xfc,0x30,0x30,0x36,0x1c,0x00], // t
    [0x00,0x00,0xcc,0xcc,0xcc,0xcc,0x76,0x00], // u
    [0x00,0x00,0xc6,0xc6,0xc6,0x6c,0x38,0x00], // v
    [0x00,0x00,0xc6,0xd6,0xd6,0xfe,0x6c,0x00], // w
    [0x00,0x00,0xc6,0x6c,0x38,0x6c,0xc6,0x00], // x
    [0x00,0x00,0xc6,0xc6,0xc6,0x7e,0x06,0xfc], // y
    [0x00,0x00,0xfe,0x8c,0x18,0x32,0xfe,0x00], // z
    [0x0e,0x18,0x18,0x70,0x18,0x18,0x0e,0x00], // {
    [0x18,0x18,0x18,0x00,0x18,0x18,0x18,0x00], // |
    [0x70,0x18,0x18,0x0e,0x18,0x18,0x70,0x00], // }
    [0x76,0xdc,0x00,0x00,0x00,0x00,0x00,0x00], // ~
];

pub struct VbeVgaDriver {
    started: AtomicBool,
    // store all mappings created for this device so we can unmap on stop
    mappings: Mutex<alloc::vec::Vec<FbMapping>>,
    // optional framebuffer info deduced after modeset
    fb_info: Mutex<Option<FramebufferInfo>>,
}

// Globals populated by `main.rs` before drivers are attached
static mut GLOBAL_MAPPER_PTR: *mut OffsetPageTable<'static> = core::ptr::null_mut();
static mut GLOBAL_ALLOC_PTR: *mut BootInfoFrameAllocator = core::ptr::null_mut();

// Runtime pointer to the active VBE driver instance (set in start, cleared in stop).
static mut ACTIVE_VBE_PTR: *mut VbeVgaDriver = core::ptr::null_mut();

/// Try to print formatted arguments to the active VBE console. Returns true if handled.
pub fn vbe_try_print(args: core::fmt::Arguments) -> bool {
    // Format into a String (alloc is available in kernel after heap init)
    use alloc::string::String;
    use core::fmt::Write;
    let mut s = String::new();
    if s.write_fmt(args).is_err() { return false; }

    // Delegate to the console driver which owns console state and will call
    // into the VBE drawing primitives. Returns true if printed to framebuffer.
    crate::driver_framework::drivers::console::console_print_first(&s)
}

/// Convenience: clear the first framebuffer console if present.
pub fn vbe_clear_first() {
    crate::driver_framework::drivers::console::console_clear_first();
}

/// Convenience: print a simple string to the first framebuffer console.
pub fn vbe_print_first(s: &str) {
    let _ = crate::driver_framework::drivers::console::console_print_first(s);
}

/// Print to the VBE framebuffer only if a framebuffer is active; do nothing
/// otherwise. This intentionally avoids falling back to the boot VGA text buffer.
pub fn vbe_print_only(s: &str) {
    let addrs = get_framebuffer_addrs();
    if addrs.is_empty() { return; }
    let _ = crate::driver_framework::drivers::console::console_print_first(s);
}

/// Set text colors for first framebuffer console
pub fn vbe_set_colors_first(fg: u32, bg: u32) {
    crate::driver_framework::drivers::console::console_set_colors_first(fg, bg);
}

/// Set cursor for first framebuffer console
pub fn vbe_set_cursor_first(col: usize, row: usize) {
    crate::driver_framework::drivers::console::console_set_cursor_first(col, row);
}

/// Convert legacy VGA text `Color` to a 32-bit ARGB color suitable for the framebuffer.
pub fn vbe_color_from_vga_color(c: crate::bootvga::vga_buffer::Color) -> u32 {
    use crate::bootvga::vga_buffer::Color;
    match c {
        Color::Black => 0xFF000000,
        Color::Blue => 0xFF0000AA,
        Color::Green => 0xFF00AA00,
        Color::Cyan => 0xFF00AAAA,
        Color::Red => 0xFFAA0000,
        Color::Magenta => 0xFFAA00AA,
        Color::Brown => 0xFFAA5500,
        Color::LightGray => 0xFFAAAAAA,
        Color::DarkGray => 0xFF555555,
        Color::LightBlue => 0xFF5555FF,
        Color::LightGreen => 0xFF55FF55,
        Color::LightCyan => 0xFF55FFFF,
        Color::LightRed => 0xFFFF5555,
        Color::Pink => 0xFFFF55FF,
        Color::Yellow => 0xFFFFFF55,
        Color::White => 0xFFFFFFFF,
    }
}

pub fn set_global_mapper_ptr(p: *mut OffsetPageTable<'static>) { unsafe { GLOBAL_MAPPER_PTR = p; } }
pub fn set_global_frame_allocator_ptr(p: *mut BootInfoFrameAllocator) { unsafe { GLOBAL_ALLOC_PTR = p; } }

impl VbeVgaDriver {
    pub fn new() -> Self {
        VbeVgaDriver {
            started: AtomicBool::new(false),
            mappings: Mutex::new(alloc::vec::Vec::new()),
            fb_info: Mutex::new(None),
        }
    }

    unsafe fn set_vbe_mode_dispi(xres: u16, yres: u16, bpp: u16) -> bool {
        const DISPI_INDEX_PORT: u16 = 0x01CE;
        const DISPI_DATA_PORT: u16 = 0x01CF;
        const DISPI_INDEX_ID: u16 = 0x0;
        const DISPI_INDEX_XRES: u16 = 0x1;
        const DISPI_INDEX_YRES: u16 = 0x2;
        const DISPI_INDEX_BPP: u16 = 0x3;
        const DISPI_INDEX_ENABLE: u16 = 0x4;
        const DISPI_DISABLED: u16 = 0x00;
        const DISPI_ENABLED: u16 = 0x01;
        const DISPI_LFB_ENABLED: u16 = 0x40;

        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_ID);
        let id = crate::arch::ports::inw(DISPI_DATA_PORT);
        if id == 0 || id == 0xFFFF { return false; }

        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_ENABLE);
        crate::arch::ports::outw(DISPI_DATA_PORT, DISPI_DISABLED);
        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_XRES);
        crate::arch::ports::outw(DISPI_DATA_PORT, xres);
        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_YRES);
        crate::arch::ports::outw(DISPI_DATA_PORT, yres);
        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_BPP);
        crate::arch::ports::outw(DISPI_DATA_PORT, bpp);
        crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_ENABLE);
        crate::arch::ports::outw(DISPI_DATA_PORT, DISPI_ENABLED | DISPI_LFB_ENABLED);
        true
    }
}

impl Driver for VbeVgaDriver {
    fn probe(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        let info = device.info();
        if info.class == 0x03 { Ok(()) } else { Err("not a display controller") }
    }

    fn start(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        if self.started.load(Ordering::SeqCst) { return Err("already started"); }
        let info = device.info();

        // Find an MMIO BAR (prefer large BARs)
        // Attempt to set a VBE mode (best-effort)
        unsafe { let _ = Self::set_vbe_mode_dispi(1024, 768, 32); }

    // We'll map every MemoryMapped BAR we find (prefer large ones) and write a test box
        let phys_mem_offset_val: u64 = crate::driver_framework::drivers::get_boot_phys_offset();

        // Prepare to store mappings
        let mut created: alloc::vec::Vec<FbMapping> = alloc::vec::Vec::new();

        // Map each MMIO BAR we discovered for this device
        for r in info.resources.iter() {
            if let ResourceKind::MemoryMapped = r.kind {
                let bar_phys = r.addr;
                let bar_len = if r.len == 0 { 0x1000u64 } else { r.len };
                let phys_map_start = bar_phys & !0xFFFu64;
                let phys_map_end = ((bar_phys + bar_len + 0xFFFu64) & !0xFFFu64);
                let page_count = ((phys_map_end - phys_map_start) / 0x1000u64) as usize;
                if page_count == 0 { continue; }

                let virt_base = phys_mem_offset_val.wrapping_add(phys_map_start);

                unsafe {
                    if GLOBAL_MAPPER_PTR.is_null() || GLOBAL_ALLOC_PTR.is_null() { return Err("mapper/alloc not set"); }
                    let mapper: &mut OffsetPageTable = &mut *GLOBAL_MAPPER_PTR;
                    let frame_alloc: &mut BootInfoFrameAllocator = &mut *GLOBAL_ALLOC_PTR;
                    for i in 0..page_count {
                        let phys = phys_map_start + (i as u64) * 0x1000u64;
                        let frame = PhysFrame::containing_address(PhysAddr::new(phys));
                        let page = Page::<Size4KiB>::containing_address(VirtAddr::new(virt_base + (i as u64) * 0x1000u64));
                        let flags = Flags::PRESENT | Flags::WRITABLE;
                        match mapper.map_to(page, frame, flags, frame_alloc) {
                            Ok(flush) => { flush.flush(); }
                            Err(_) => { break; }
                        }
                    }
                }

                created.push(FbMapping { virt_base, phys_map_start, bar_phys, pages: page_count });
            }
        }

        if created.is_empty() { return Err("no MMIO BARs mapped"); }

        // Read back resolution/BPP from DISPI registers (best-effort) and store in fb_info
        unsafe {
            // DISPI registers
            const DISPI_INDEX_PORT: u16 = 0x01CE;
            const DISPI_DATA_PORT: u16 = 0x01CF;
            const DISPI_INDEX_XRES: u16 = 0x1;
            const DISPI_INDEX_YRES: u16 = 0x2;
            const DISPI_INDEX_BPP: u16 = 0x3;
            crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_XRES);
            let xres = crate::arch::ports::inw(DISPI_DATA_PORT) as u32;
            crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_YRES);
            let yres = crate::arch::ports::inw(DISPI_DATA_PORT) as u32;
            crate::arch::ports::outw(DISPI_INDEX_PORT, DISPI_INDEX_BPP);
            let bpp = crate::arch::ports::inw(DISPI_DATA_PORT) as u32;
            if xres != 0 && yres != 0 {
                let pitch = (xres as usize) * ((bpp as usize + 7) / 8);
                *self.fb_info.lock() = Some(FramebufferInfo { width: xres, height: yres, bpp, pitch });
            }
        }

        // Save mappings on the struct for later unmap (move created)
        *self.mappings.lock() = created;
        // Mark driver as active for global helpers
        unsafe { ACTIVE_VBE_PTR = (self as *const VbeVgaDriver) as *mut VbeVgaDriver; }

        self.started.store(true, Ordering::SeqCst);
        Ok(())
    }

    fn stop(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        if !self.started.load(Ordering::SeqCst) { return; }

        // Clear the test box for each mapping and unmap pages
        let mut mappings = self.mappings.lock();
        if !mappings.is_empty() {
            // Clear box in each mapping
            let pitch = 1024 * 4;
            for m in mappings.iter() {
                unsafe {
                    let fb = m.virt_base + (m.bar_phys - m.phys_map_start);
                    let fb_ptr = fb as *mut u8;
                    if !fb_ptr.is_null() {
                        for y in 100usize..300usize {
                            let row = fb_ptr.add(y * pitch);
                            for x in 100usize..300usize {
                                ptr::write_volatile(row.add(x * 4) as *mut u32, 0u32);
                            }
                        }
                    }
                }
            }

            // Unmap pages
            unsafe {
                if !GLOBAL_MAPPER_PTR.is_null() {
                    let mapper: &mut OffsetPageTable = &mut *GLOBAL_MAPPER_PTR;
                    for m in mappings.iter() {
                        for i in 0..m.pages {
                            let page = Page::<Size4KiB>::containing_address(VirtAddr::new(m.virt_base + (i as u64) * 0x1000u64));
                            let _ = mapper.unmap(page);
                        }
                    }
                }
            }

            // clear stored mappings
            *mappings = alloc::vec::Vec::new();
        }

        self.started.store(false, Ordering::SeqCst);
        // clear active pointer if we were the active driver
        unsafe {
            if !ACTIVE_VBE_PTR.is_null() && ACTIVE_VBE_PTR == (self as *const VbeVgaDriver) as *mut VbeVgaDriver {
                ACTIVE_VBE_PTR = core::ptr::null_mut();
            }
        }
    }

    fn release(&self, _device: &crate::driver_framework::device::DeviceHandle) { self.stop(_device); }
}

pub fn boxed_driver() -> Box<dyn Driver> { Box::new(VbeVgaDriver::new()) }

// Boot-phys offset helper (set by main.rs)
static BOOT_PHYS_OFFSET_GLOBAL: Mutex<u64> = Mutex::new(0);
pub fn set_boot_phys_offset(val: u64) { *BOOT_PHYS_OFFSET_GLOBAL.lock() = val; }
pub fn get_boot_phys_offset() -> u64 { *BOOT_PHYS_OFFSET_GLOBAL.lock() }

// Module-level safe-ish wrappers that delegate to the active VBE driver instance.
// These avoid exposing internals and provide a small API for other modules (console).
pub fn get_framebuffer_addrs() -> alloc::vec::Vec<u64> {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return alloc::vec::Vec::new(); }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        drv.get_framebuffer_addrs()
    }
}

pub fn get_fb_info() -> Option<FramebufferInfo> {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return None; }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        *drv.fb_info.lock()
    }
}

pub fn draw_pixel_at(fb_virt: u64, x: usize, y: usize, color: u32) {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return; }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        drv.draw_pixel_at(fb_virt, x, y, color);
    }
}

pub fn draw_rect_at(fb_virt: u64, x: usize, y: usize, w: usize, h: usize, color: u32) {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return; }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        drv.draw_rect_at(fb_virt, x, y, w, h, color);
    }
}

pub fn draw_char_at(fb_virt: u64, x: usize, y: usize, ch: u8, color: u32) {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return; }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        drv.draw_char_at(fb_virt, x, y, ch, color);
    }
}

pub fn draw_text_absolute(fb_virt: u64, x: usize, y: usize, s: &str, color: u32) {
    unsafe {
        if ACTIVE_VBE_PTR.is_null() { return; }
        let drv: &VbeVgaDriver = &*ACTIVE_VBE_PTR;
        drv.draw_text_absolute(fb_virt, x, y, s, color);
    }
}

// --- Drawing / text helpers ---
impl VbeVgaDriver {
    /// Return a vector of framebuffer virtual addresses for each mapped BAR.
    /// Each entry is the virtual base corresponding to the BAR's start (virt_base + (bar_phys - phys_map_start)).
    pub fn get_framebuffer_addrs(&self) -> alloc::vec::Vec<u64> {
        let mut out = alloc::vec::Vec::new();
        for m in self.mappings.lock().iter() {
            out.push(m.virt_base + (m.bar_phys - m.phys_map_start));
        }
        out
    }

    /// Draw a single pixel to a framebuffer virtual address.
    pub fn draw_pixel_at(&self, fb_virt: u64, x: usize, y: usize, color: u32) {
        let pitch = if let Some(info) = *self.fb_info.lock() { info.pitch } else { 1024usize * 4 };
        unsafe {
            let base = fb_virt as *mut u8;
            let row = base.add(y * pitch);
            let p = row.add(x * 4) as *mut u32;
            ptr::write_volatile(p, color);
        }
    }

    /// Draw a filled rectangle to a framebuffer virtual address. Assumes ARGB32.
    pub fn draw_rect_at(&self, fb_virt: u64, x: usize, y: usize, w: usize, h: usize, color: u32) {
        let pitch = if let Some(info) = *self.fb_info.lock() { info.pitch } else { 1024usize * 4 };
        unsafe {
            let base = fb_virt as *mut u8;
            for yy in y..(y + h) {
                let row = base.add(yy * pitch);
                for xx in x..(x + w) {
                    ptr::write_volatile((row.add(xx * 4) as *mut u32), color);
                }
            }
        }
    }

    /// Draw a single 8x8 character using a simple procedural glyph generator.
    /// This is a fallback visible glyph (not an accurate VGA ROM font). If you want
    /// a full font, we can embed a font table or implement a VGA font loader.
    pub fn draw_char_at(&self, fb_virt: u64, x: usize, y: usize, ch: u8, color: u32) {
        // Use embedded VGA 8x8 font when available; fallback to procedural glyph otherwise.
        let pitch = if let Some(info) = *self.fb_info.lock() { info.pitch } else { 1024usize * 4 };
        // Attempt to read font data
        if let Some(glyph) = VGA8X8::get_glyph(ch) {
            unsafe {
                let base = fb_virt as *mut u8;
                for r in 0..8usize {
                    let row = base.add((y + r) * pitch);
                    let bits = glyph[r];
                    for c in 0..8usize {
                        if (bits & (1 << (7 - c))) != 0 {
                            ptr::write_volatile((row.add((x + c) * 4) as *mut u32), color);
                        }
                    }
                }
            }
            return;
        }

        // Fallback procedural glyph
        unsafe {
            let base = fb_virt as *mut u8;
                for r in 0..8usize {
                    let row = base.add((y + r) * pitch);
                    let mut pattern: u8 = ((ch.wrapping_add(r as u8)) ^ (ch >> (r % 8))) as u8;
                    pattern = pattern.rotate_left((r as u32) & 7);
                    for c in 0..8usize {
                        if (pattern & (1 << c)) != 0 {
                            ptr::write_volatile((row.add((x + c) * 4) as *mut u32), color);
                        }
                    }
                }
        }
    }
    /// Keep the old absolute text drawing API if needed.
    pub fn draw_text_absolute(&self, fb_virt: u64, x: usize, y: usize, s: &str, color: u32) {
        let mut cx = x;
        // Character cell width (8px glyph + 1px spacing)
        let cw = 9usize;
        for b in s.bytes() {
            if b == b'\n' { continue; }
            self.draw_char_at(fb_virt, cx, y, b, color);
            cx += cw;
        }
    }
}
