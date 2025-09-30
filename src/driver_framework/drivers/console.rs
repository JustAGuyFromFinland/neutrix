use crate::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr;
use core::fmt::Write;
use spin::Mutex;
use crate::driver_framework::driver::Driver;

// A per-framebuffer Console object moved out of the VBE driver. It holds
// cursor position, colors and text metrics and calls into the VBE drawing
// primitives exposed by `vbe_vga`.
struct Console {
    fb_virt: u64,
    cols: usize,
    rows: usize,
    cur_x: usize,
    cur_y: usize,
    fg: u32,
    bg: u32,
    char_w: usize,
    char_h: usize,
}

impl Console {
    fn newline(&mut self) {
        self.cur_x = 0;
        self.cur_y += 1;
    }
}

/// Simple manager storing Console objects (one per framebuffer).
static CONSOLES: Mutex<Vec<Console>> = Mutex::new(Vec::new());

/// Find or create a console for a given framebuffer virtual address.
fn get_or_create_console(fb_virt: u64) -> usize {
    let mut consoles = CONSOLES.lock();
    for (i, c) in consoles.iter().enumerate() {
        if c.fb_virt == fb_virt { return i; }
    }
    // create from fb_info if available
    let mut cols = 80usize;
    let mut rows = 25usize;
    let mut char_w = 9usize;
    let mut char_h = 8usize;
    if let Some(info) = crate::driver_framework::drivers::vbe_vga::get_fb_info() {
        char_w = 9;
        char_h = 8;
        cols = (info.width as usize) / char_w;
        rows = (info.height as usize) / char_h;
        if cols == 0 { cols = 80; }
        if rows == 0 { rows = 25; }
    }
    let c = Console { fb_virt, cols, rows, cur_x: 0, cur_y: 0, fg: 0xFFFFFFFFu32, bg: 0x00000000u32, char_w, char_h };
    consoles.push(c);
    consoles.len() - 1
}

/// Public helper: print a string to the first framebuffer console (best-effort).
pub fn console_print_first(s: &str) -> bool {
    use crate::driver_framework::drivers::vbe_vga;
    let addrs = vbe_vga::get_framebuffer_addrs();
    if addrs.is_empty() {
        // fallback to boot VGA
        crate::bootvga::vga_buffer::WRITER.lock().write_str(s).ok();
        return false;
    }
    // Use the first FB
    let fb = addrs[0];
    let idx = get_or_create_console(fb);
    let mut consoles = CONSOLES.lock();
    let mut console = consoles.remove(idx);
    // Write bytes with handling for newline/tab/backspace
    for b in s.bytes() {
        match b {
            b'\n' => {
                console.newline();
                if console.cur_y >= console.rows {
                    // scroll up one row
                    console_scroll_mut(&mut console, 1);
                    console.cur_y = console.rows - 1;
                }
            }
            b'\r' => { console.cur_x = 0; }
            8u8 => { // backspace
                if console.cur_x > 0 { console.cur_x -= 1; } else if console.cur_y > 0 { console.cur_y -= 1; console.cur_x = console.cols.saturating_sub(1); }
                let px = (console.cur_x * console.char_w) as usize;
                let py = (console.cur_y * console.char_h) as usize;
                crate::driver_framework::drivers::vbe_vga::draw_rect_at(fb, px, py, console.char_w, console.char_h, console.bg);
            }
            9u8 => { // tab
                let tab_width = 8usize;
                let next = ((console.cur_x / tab_width) + 1) * tab_width;
                if next >= console.cols { console.newline(); } else { console.cur_x = next; }
            }
            _ => {
                let px = (console.cur_x * console.char_w) as usize;
                let py = (console.cur_y * console.char_h) as usize;
                crate::driver_framework::drivers::vbe_vga::draw_char_at(fb, px, py, b, console.fg);
                console.cur_x += 1;
                if console.cur_x >= console.cols {
                    console.newline();
                    if console.cur_y >= console.rows {
                        console_scroll_mut(&mut console, 1);
                        console.cur_y = console.rows - 1;
                    }
                }
            }
        }
    }
    consoles.insert(idx, console);
    true
}

/// Scroll mutating helper, similar to previous implementation in VBE driver.
fn console_scroll_mut(console: &mut Console, lines: usize) {
    if lines == 0 { return; }
    let pitch = if let Some(info) = crate::driver_framework::drivers::vbe_vga::get_fb_info() { info.pitch } else { 1024usize * 4 };
    if lines >= console.rows {
        crate::driver_framework::drivers::vbe_vga::draw_rect_at(console.fb_virt, 0, 0, console.cols * console.char_w, console.rows * console.char_h, console.bg);
        console.cur_x = 0; console.cur_y = 0; return;
    }
    let move_height = (console.rows - lines) * console.char_h;
    let src_offset = lines * console.char_h * pitch;
    let move_bytes = move_height * pitch;
    unsafe {
        let base = console.fb_virt as *mut u8;
        let src = base.add(src_offset);
        let dst = base.add(0);
        core::ptr::copy(src, dst, move_bytes);
        // clear last `lines` rows
        let clear_start = (console.rows - lines) * console.char_h * pitch;
        let clear_bytes = lines * console.char_h * pitch;
        let mut p = base.add(clear_start);
        let end = p.add(clear_bytes);
        while p < end {
            if (end as usize).wrapping_sub(p as usize) >= 4 {
                core::ptr::write_volatile(p as *mut u32, console.bg);
                p = p.add(4);
            } else {
                core::ptr::write_volatile(p, 0u8);
                p = p.add(1);
            }
        }
    }
}

/// Public helper: clear first console if present
pub fn console_clear_first() {
    use crate::driver_framework::drivers::vbe_vga;
    let addrs = vbe_vga::get_framebuffer_addrs();
    if addrs.is_empty() { crate::bootvga::vga_buffer::WRITER.lock().clear_screen(); return; }
    let fb = addrs[0];
    let idx = get_or_create_console(fb);
    let mut consoles = CONSOLES.lock();
    let mut c = consoles.remove(idx);
    crate::driver_framework::drivers::vbe_vga::draw_rect_at(c.fb_virt, 0, 0, c.cols * c.char_w, c.rows * c.char_h, c.bg);
    c.cur_x = 0; c.cur_y = 0;
    consoles.insert(idx, c);
}

pub fn console_set_colors_first(fg: u32, bg: u32) {
    use crate::driver_framework::drivers::vbe_vga;
    let addrs = vbe_vga::get_framebuffer_addrs();
    if addrs.is_empty() { return; }
    let fb = addrs[0];
    let idx = get_or_create_console(fb);
    let mut consoles = CONSOLES.lock();
    if let Some(c) = consoles.get_mut(idx) { c.fg = fg; c.bg = bg; }
}

pub fn console_set_cursor_first(col: usize, row: usize) {
    use crate::driver_framework::drivers::vbe_vga;
    let addrs = vbe_vga::get_framebuffer_addrs();
    if addrs.is_empty() { return; }
    let fb = addrs[0];
    let idx = get_or_create_console(fb);
    let mut consoles = CONSOLES.lock();
    if let Some(c) = consoles.get_mut(idx) {
        c.cur_x = core::cmp::min(col, c.cols.saturating_sub(1));
        c.cur_y = core::cmp::min(row, c.rows.saturating_sub(1));
    }
}

/// The driver itself is a thin logical device implementer; console state is global/static.
pub struct ConsoleDriver {}

impl ConsoleDriver { pub fn new() -> Self { ConsoleDriver {} } }

impl Driver for ConsoleDriver {
    fn probe(&self, _device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> { Ok(()) }
    fn start(&self, _device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> { Ok(()) }
    fn stop(&self, _device: &crate::driver_framework::device::DeviceHandle) {}
    fn release(&self, _device: &crate::driver_framework::device::DeviceHandle) {}
}

pub fn boxed_driver() -> Box<dyn Driver> { Box::new(ConsoleDriver::new()) }
