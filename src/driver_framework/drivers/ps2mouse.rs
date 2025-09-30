use crate::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicU8, Ordering};
use spin::Mutex;
use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use futures_util::task::AtomicWaker;
use core::pin::Pin;
use core::task::Poll;
use futures_util::stream::Stream;
use futures_util::StreamExt;
use x86_64::structures::idt::InterruptStackFrame;

use crate::driver_framework::driver::Driver;
use crate::driver_framework::device::{DeviceInfo, Resource, ResourceKind};
// (No global debug counters)

/// Simple PS/2 mouse driver that registers an IRQ handler and tracks a small
/// software cursor drawn into the VBE framebuffer.
pub struct Ps2MouseDriver {
    registered_vectors: Mutex<Vec<u8>>,
    // packet state: collect 3-byte PS/2 packets
    pkt_state: AtomicU8, // 0..=2 current index
    pkt_buf: Mutex<[u8;3]>,
    // current cursor position (in pixels)
    cursor_x: Mutex<i32>,
    cursor_y: Mutex<i32>,
    // (no target smoothing; movement applied immediately)
    // saved background pixels under the cursor (bx, by, vec row-major ARGB32)
    saved_bg: Mutex<Option<(usize, usize, alloc::vec::Vec<u32>)>>,
}

impl Ps2MouseDriver {
    pub fn new() -> Self {
        Ps2MouseDriver {
            registered_vectors: Mutex::new(Vec::new()),
            pkt_state: AtomicU8::new(0),
            pkt_buf: Mutex::new([0u8;3]),
            cursor_x: Mutex::new(40),
            cursor_y: Mutex::new(40),
            // no targets
            saved_bg: Mutex::new(None),
        }
    }

    extern "x86-interrupt" fn irq_handler(_stack_frame: InterruptStackFrame) {
        use x86_64::instructions::port::Port;
        let mut port = Port::new(0x60);
        let b: u8 = unsafe { port.read() };

        // SAFETY: this is a static handler; use the typed global instance
        if let Some(drv) = crate::driver_framework::drivers::ps2mouse::get_global_instance_typed() {
            // Simple state machine: 0 = expect header, 1 = X, 2 = Y
            let state = drv.pkt_state.load(Ordering::SeqCst) as u8;
            match state {
                0 => {
                    // Validate header: bit 3 of first byte should be 1 (always 1 per PS/2 spec).
                    if (b & 0x08) == 0 {
                        // Not a valid packet start; drop the byte and stay in state 0.
                    } else {
                        let mut buf = drv.pkt_buf.lock();
                        buf[0] = b;
                        drv.pkt_state.store(1, Ordering::SeqCst);
                    }
                }
                1 => {
                    let mut buf = drv.pkt_buf.lock();
                    buf[1] = b;
                    drv.pkt_state.store(2, Ordering::SeqCst);
                }
                2 => {
                    let mut buf = drv.pkt_buf.lock();
                    buf[2] = b;
                    // Full packet ready: extract and push to queue
                    let buttons = buf[0];
                    let dx = buf[1] as i8;
                    let dy = buf[2] as i8;
                    // reset state
                    drv.pkt_state.store(0, Ordering::SeqCst);

                    // Push packet into cross-thread queue for non-IRQ processing
                    if let Ok(q) = MOUSE_QUEUE.try_get() {
                        let _ = q.push(MousePacket { buttons, dx, dy });
                        // Also wake any waiters
                        MOUSE_WAKER.wake();
                    }
                }
                _ => {
                    drv.pkt_state.store(0, Ordering::SeqCst);
                }
            }
            // Avoid heavy work (drawing/alloc) in IRQ context
        }

        unsafe {
            if crate::hal::apic::is_initialized() {
                crate::hal::apic::send_eoi();
            } else {
                let vec = GLOBAL_PS2MOUSE_VECTOR.load(Ordering::SeqCst);
                crate::arch::interrupts::PICS.lock().notify_end_of_interrupt(vec);
            }
        }
    }

    fn redraw_cursor(&self) {
        use crate::driver_framework::drivers::vbe_vga;
        let addrs = vbe_vga::get_framebuffer_addrs();
        if addrs.is_empty() { return; }
        let fb = addrs[0];

        // simple 12x16 monochrome arrow bitmap (1 = pixel on)
        const W: usize = 12;
        const H: usize = 16;
        static ARROW: [u16; H] = [
            0b100000000000,
            0b110000000000,
            0b111000000000,
            0b111100000000,
            0b111110000000,
            0b111111000000,
            0b111111100000,
            0b111111110000,
            0b111111100000,
            0b111111000000,
            0b111110000000,
            0b111100000000,
            0b111000000000,
            0b110000000000,
            0b100000000000,
            0b000000000000,
        ];

        let x = *self.cursor_x.lock();
        let y = *self.cursor_y.lock();

        // clamp to reasonable bounds using fb_info
        if let Some(info) = vbe_vga::get_fb_info() {
            let max_x = info.width as i32 - (W as i32);
            let max_y = info.height as i32 - (H as i32);
            let mut cx = x; if cx < 0 { cx = 0; } if cx > max_x { cx = max_x; }
            let mut cy = y; if cy < 0 { cy = 0; } if cy > max_y { cy = max_y; }

            let bx = cx as usize; let by = cy as usize;
            // First, restore previous background if exists
            {
                let mut saved = self.saved_bg.lock();
                if let Some((px, py, ref vec)) = *saved {
                    // If previously saved area differs from new area, restore and clear saved
                    if px != bx || py != by {
                        // restore pixel-wise
                        let mut i = 0usize;
                        for ry in 0..H {
                            for rx in 0..W {
                                vbe_vga::draw_pixel_at(fb, px + rx, py + ry, vec[i]);
                                i += 1;
                            }
                        }
                        *saved = None;
                    }
                }
            }

            // Capture current background under new cursor position into a vector
            let mut bg: alloc::vec::Vec<u32> = alloc::vec::Vec::with_capacity(W * H);
            bg.resize(W * H, 0u32);
            // Read pixels by reading memory from framebuffer mapping directly
            // Unfortunately vbe_vga doesn't expose a read API; emulate by reading via volatile pointer
            if let Some(info) = vbe_vga::get_fb_info() {
                let pitch = info.pitch;
                unsafe {
                    let base = fb as *mut u8;
                    let mut idx = 0usize;
                    for ry in 0..H {
                        let row = base.add((by + ry) * pitch);
                        for rx in 0..W {
                            let p = row.add((bx + rx) * 4) as *mut u32;
                            let val = core::ptr::read_volatile(p);
                            bg[idx] = val;
                            idx += 1;
                        }
                    }
                }
            }

            // Save captured background
            *self.saved_bg.lock() = Some((bx, by, bg));

            // draw arrow pixels in white over captured background
            for row in 0..H {
                let bits = ARROW[row];
                for col in 0..W {
                    if (bits & (1 << (W - 1 - col))) != 0 {
                        vbe_vga::draw_pixel_at(fb, bx + col, by + row, 0xFFFFFFFFu32);
                    }
                }
            }
        }
    }

    // (Removed light-dot cursor; arrow redraw is used for smooth movement)

    // Helper: read controller status port (0x64)
    fn read_status(&self) -> u8 {
        use x86_64::instructions::port::Port;
        let mut p: Port<u8> = Port::new(0x64);
        unsafe { p.read() }
    }

    // Helper: read data port (0x60) if output buffer has data
    fn read_data(&self) -> u8 {
        use x86_64::instructions::port::Port;
        let mut p: Port<u8> = Port::new(0x60);
        unsafe { p.read() }
    }

    // Wait until input buffer clear (controller ready to accept command/data)
    // Returns true on success, false on timeout
    fn wait_input_clear(&self, max_loops: usize) -> bool {
        for _ in 0..max_loops {
            if (self.read_status() & 0x02) == 0 { return true; }
        }
        false
    }

    // Wait for output buffer to have data and return it (with timeout)
    fn wait_for_data(&self, max_loops: usize) -> Option<u8> {
        for _ in 0..max_loops {
            if (self.read_status() & 0x01) != 0 {
                return Some(self.read_data());
            }
        }
        None
    }

    // Send a byte to the controller (0x64) as a command. Wait for input buffer clear first.
    fn write_controller_cmd(&self, cmd: u8) -> bool {
        use x86_64::instructions::port::Port;
        if !self.wait_input_clear(10000) { return false; }
        let mut p: Port<u8> = Port::new(0x64);
        unsafe { p.write(cmd); }
        true
    }

    // Write a byte to the controller data port (0x60). Wait for input clear first.
    fn write_controller_data(&self, data: u8) -> bool {
        use x86_64::instructions::port::Port;
        if !self.wait_input_clear(10000) { return false; }
        let mut p: Port<u8> = Port::new(0x60);
        unsafe { p.write(data); }
        true
    }

    // Read the controller command byte (0x20 command). Returns Some(byte) on success.
    fn read_controller_config(&self) -> Option<u8> {
        // Issue command 0x20 to read command byte
        if !self.write_controller_cmd(0x20) { return None; }
        // Wait for data in output buffer
        self.wait_for_data(10000)
    }

    // Send a mouse-targeted byte: tell controller 0xD4 then write data to 0x60.
    fn write_mouse_data(&self, data: u8) -> bool {
        use x86_64::instructions::port::Port;
        // Send 0xD4 command to controller to forward next byte to mouse
        if !self.write_controller_cmd(0xD4) { return false; }
        // Wait input clear then write data
        if !self.wait_input_clear(10000) { return false; }
        let mut p: Port<u8> = Port::new(0x60);
        unsafe { p.write(data); }
        true
    }

    // Send a mouse command and wait for ACK (0xFA). Handles 0xFE (resend) automatically
    // Retries the full sequence up to "retries" times. Returns true on ACK.
    fn send_mouse_cmd_with_ack(&self, cmd: u8, retries: usize) -> bool {
        for attempt in 0..retries {
            if !self.write_mouse_data(cmd) {
                // failed to write; try again
                continue;
            }

            // Wait for ACK/response
            if let Some(resp) = self.wait_for_data(10000) {
                if resp == 0xFA { return true; } // ACK
                if resp == 0xFE {
                    // Resend requested by device, retry
                    if attempt + 1 >= retries { break; } else { continue; }
                }
                // Other responses (e.g., 0xFC) treat as failure and retry
            } else {
                // Timeout waiting for response; retry
            }
        }
        false
    }
}

// (No debug logging in this driver build)

// --- IRQ-safe queue and async stream for mouse packets ---
#[derive(Clone, Copy, Debug)]
pub struct MousePacket { buttons: u8, dx: i8, dy: i8 }

static MOUSE_QUEUE: OnceCell<ArrayQueue<MousePacket>> = OnceCell::uninit();
static MOUSE_WAKER: AtomicWaker = AtomicWaker::new();

pub struct MousePacketStream { _private: () }
impl MousePacketStream {
    pub fn new() -> Self { MOUSE_QUEUE.try_init_once(|| ArrayQueue::new(256)).ok(); MousePacketStream { _private: () } }
}
impl Stream for MousePacketStream {
    type Item = MousePacket;
    fn poll_next(self: Pin<&mut Self>, cx: &mut core::task::Context) -> Poll<Option<MousePacket>> {
        let q = MOUSE_QUEUE.try_get().expect("mouse queue not initialized");
        if let Some(pkt) = q.pop() { return Poll::Ready(Some(pkt)); }
        MOUSE_WAKER.register(&cx.waker());
        match q.pop() { Some(p) => { MOUSE_WAKER.take(); Poll::Ready(Some(p)) } None => Poll::Pending }
    }
}

/// Async task that processes mouse packets off the IRQ and updates cursor/draws.
pub async fn mouse_event_loop() {
    let mut stream = MousePacketStream::new();
    let mut count: usize = 0;
    // Movement tuning parameters: adjust sensitivity, maximum per-packet delta
    const MOUSE_SENS_NUM: i32 = 1; // numerator for sensitivity multiplier
    const MOUSE_SENS_DEN: i32 = 1; // denominator for sensitivity multiplier
    const MOUSE_MAX_DELTA: i32 = 16; // clamp per-packet delta to this range
    const MOUSE_INVERT_Y: bool = false; // if true, invert vertical axis

    while let Some(pkt) = stream.next().await {
        // Diagnostic: print every packet (throttled by count to avoid spam)
        count = count.wrapping_add(1);
        // (No debug printing)

        // Move cursor and perform lightweight redraw on every packet.
        if let Some(drv) = crate::driver_framework::drivers::ps2mouse::get_global_instance_typed() {
            // Normalize and clamp packet deltas, apply sensitivity and optional inversion.
            let mut dx = pkt.dx as i32;
            let mut dy = pkt.dy as i32;
            if dx > MOUSE_MAX_DELTA { dx = MOUSE_MAX_DELTA } else if dx < -MOUSE_MAX_DELTA { dx = -MOUSE_MAX_DELTA }
            if dy > MOUSE_MAX_DELTA { dy = MOUSE_MAX_DELTA } else if dy < -MOUSE_MAX_DELTA { dy = -MOUSE_MAX_DELTA }
            // Apply sensitivity scaling
            dx = dx * MOUSE_SENS_NUM / MOUSE_SENS_DEN;
            dy = dy * MOUSE_SENS_NUM / MOUSE_SENS_DEN;
            // Convert device Y (positive = up) to screen Y (positive = down) by negating
            let screen_dy = if MOUSE_INVERT_Y { dy } else { -dy };
            // Apply movement immediately to displayed cursor
            {
                let mut x = drv.cursor_x.lock();
                let mut y = drv.cursor_y.lock();
                *x = (*x).saturating_add(dx);
                *y = (*y).saturating_add(screen_dy);
            }
            // Redraw cursor at new position
            drv.redraw_cursor();
            // Movement applied immediately; the outer stream await will park when the queue is empty.
        }
    }
}

impl Driver for Ps2MouseDriver {
    fn probe(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        let info = device.info();
        if info.class == 0x09 || info.description.contains("PS/2 Mouse") || info.description.contains("Mouse") {
            Ok(())
        } else { Err("not a PS/2 mouse") }
    }

    fn start(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        let info = device.info();
        for r in info.resources.iter() {
            if let ResourceKind::Interrupt(vector) = r.kind {
                crate::arch::idt::register_irq_handler(vector, Ps2MouseDriver::irq_handler);
                let mut reg = self.registered_vectors.lock();
                if !reg.contains(&vector) { reg.push(vector); }
                // store vector for PIC EOI fallback
                GLOBAL_PS2MOUSE_VECTOR.store(vector, Ordering::SeqCst);
            }
        }
        // register a typed global instance pointer so the IRQ handler can find us
        crate::driver_framework::drivers::ps2mouse::set_global_instance(self as *const _ as *mut Ps2MouseDriver);
        // Robust PS/2 init: enable aux port, then send 0xF4 (Enable Data Reporting) to the mouse and wait for ACK.
        // Retry the sequence a few times if we get a resend (0xFE) or timeouts.
        // This approach polls the controller input/output buffer bits (0x64 status port).

        // Enable auxiliary device (second PS/2 port)
        let _ = self.write_controller_cmd(0xA8);

        // Flush any pending output bytes before starting
        while (self.read_status() & 0x01) != 0 {
            let _ = self.read_data();
        }

        // Ensure the controller command byte enables mouse IRQs (bit 1).
        if let Some(cfg) = self.read_controller_config() {
            let want = cfg | 0x02u8; // set bit1 = enable aux/mouse IRQ
            if want != cfg {
                // Write back via command 0x60 then data port
                if self.write_controller_cmd(0x60) {
                    let _ = self.write_controller_data(want);
                } else {
                    // ignore write failure
                }
            }
        } else {
            // ignore read failure
        }

        // Try sending Enable Data Reporting (0xF4) with ACK polling
        let success = self.send_mouse_cmd_with_ack(0xF4u8, 4);
        let _ = success;

        // Diagnostic: print that start completed and which vector we registered (if any)
        let vec = GLOBAL_PS2MOUSE_VECTOR.load(Ordering::SeqCst);
    let _ = vec;
        Ok(())
    }

    fn stop(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        let reg = self.registered_vectors.lock();
        for &v in reg.iter() { crate::arch::idt::unregister_irq_handler(v); }
    }

    fn release(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        let mut reg = self.registered_vectors.lock();
        for &v in reg.iter() { crate::arch::idt::unregister_irq_handler(v); }
        reg.clear();
        crate::driver_framework::drivers::ps2mouse::set_global_instance(core::ptr::null_mut());
    }
}

pub fn boxed_driver() -> Box<dyn Driver> { Box::new(Ps2MouseDriver::new()) }

// Typed global instance pointer so the static IRQ handler can access the driver object.
static mut GLOBAL_PS2MOUSE_INSTANCE: *mut Ps2MouseDriver = core::ptr::null_mut();
use core::sync::atomic::AtomicU8 as AtomicU8_local;
static GLOBAL_PS2MOUSE_VECTOR: AtomicU8_local = AtomicU8_local::new(0);
pub fn set_global_instance(p: *mut Ps2MouseDriver) { unsafe { GLOBAL_PS2MOUSE_INSTANCE = p; } }
pub fn get_global_instance_typed() -> Option<&'static Ps2MouseDriver> {
    unsafe {
        if GLOBAL_PS2MOUSE_INSTANCE.is_null() { return None; }
        Some(&*GLOBAL_PS2MOUSE_INSTANCE)
    }
}

/// Public helper to set cursor position from outside (e.g., main.rs)
pub fn set_cursor_pos(x: i32, y: i32) {
    unsafe {
        if GLOBAL_PS2MOUSE_INSTANCE.is_null() { return; }
        let drv = &*GLOBAL_PS2MOUSE_INSTANCE;
        *drv.cursor_x.lock() = x;
        *drv.cursor_y.lock() = y;
        drv.redraw_cursor();
    }
}
