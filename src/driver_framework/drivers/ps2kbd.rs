use crate::*;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::pin::Pin;
use core::task::Poll;
use futures_util::stream::Stream;
use futures_util::task::AtomicWaker;
use core::sync::atomic::Ordering as AtomicOrdering;
use futures_util::StreamExt;
use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use pc_keyboard::*;
use x86_64::structures::idt::InterruptStackFrame;
use spin::Mutex;

use crate::driver_framework::driver::Driver;
use crate::driver_framework::device::{DeviceInfo, Resource, ResourceKind};

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

pub struct Ps2KbdDriver {
    /// Tracks which IRQ vectors this driver registered so they can be
    /// unregistered on stop/release. Protected by a spin::Mutex because
    /// Driver methods take `&self`.
    registered_vectors: Mutex<Vec<u8>>,
}

impl Ps2KbdDriver {
    pub fn new() -> Self {
        Ps2KbdDriver { registered_vectors: Mutex::new(Vec::new()) }
    }

    fn init_queue_if_needed(&self) {
        SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100)).ok();
    }

    extern "x86-interrupt" fn irq_handler(_stack_frame: InterruptStackFrame) {
        use x86_64::instructions::port::Port;
        let mut port = Port::new(0x60);
        let scancode: u8 = unsafe { port.read() };
        if let Ok(queue) = SCANCODE_QUEUE.try_get() {
            let _ = queue.push(scancode);
            WAKER.wake();
        }
        unsafe {
            if crate::hal::apic::is_initialized() {
                crate::hal::apic::send_eoi();
            } else {
                crate::arch::interrupts::PICS.lock().notify_end_of_interrupt(crate::arch::interrupts::InterruptIndex::Keyboard.as_u8());
            }
        }
    }
}

impl Driver for Ps2KbdDriver {
    fn probe(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        let info = device.info();
        // Match by class = Input Device or a custom description match
        if info.class == 0x09 || info.description.contains("PS/2 Keyboard") {
            Ok(())
        } else {
            Err("not a PS/2 keyboard")
        }
    }

    fn start(&self, device: &crate::driver_framework::device::DeviceHandle) -> Result<(), &'static str> {
        // Initialize queues and register IRQ handler on the IDT for the vector
        self.init_queue_if_needed();
        // The device resources may include an Interrupt entry with the vector
        let info = device.info();
        for r in info.resources.iter() {
            if let ResourceKind::Interrupt(vector) = r.kind {
                // Register IRQ handler now so the kernel can receive scancodes when
                // the controller/port is enabled. Keep the vector in our registered
                // list so we can unregister on stop/release.
                crate::arch::idt::register_irq_handler(vector, Ps2KbdDriver::irq_handler);
                let mut reg = self.registered_vectors.lock();
                if !reg.contains(&vector) { reg.push(vector); }
            }
        }
        // Start with keyboard port disabled by default so callers must enable it
        // explicitly (e.g., getline). Use PS/2 controller command 0xAD.
        {
            use x86_64::instructions::port::Port;
            // Wait until input buffer clear then send 0xAD
            let mut status_port: Port<u8> = Port::new(0x64);
            // simple spin wait (small) to avoid blocking too long
            for _ in 0..10000 { if (unsafe { status_port.read() } & 0x02) == 0 { break; } }
            let mut cmd_port: Port<u8> = Port::new(0x64);
            unsafe { cmd_port.write(0xADu8); }
            crate::driver_framework::drivers::console::console_print_first("[kbd] PS/2 keyboard port disabled by default at start()\n");
        }
        Ok(())
    }

    fn stop(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        // Unregister any IRQ handlers registered by this driver. Keep the
        // vector list so a later `release` call is idempotent.
        let reg = self.registered_vectors.lock();
        for &v in reg.iter() {
            crate::arch::idt::unregister_irq_handler(v);
        }
    }

    fn release(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        // Fully release resources and clear our registered vector list.
        let mut reg = self.registered_vectors.lock();
        for &v in reg.iter() {
            crate::arch::idt::unregister_irq_handler(v);
        }
        reg.clear();
    }
}

pub fn boxed_driver() -> Box<dyn Driver> {
    Box::new(Ps2KbdDriver::new())
}

// Provide a small async stream API for consumers (getline/print_keypresses) to use
pub struct ScancodeStream { _private: () }
impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100)).ok();
        ScancodeStream { _private: () }
    }
}
impl Stream for ScancodeStream {
    type Item = u8;
    fn poll_next(self: Pin<&mut Self>, cx: &mut core::task::Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE.try_get().expect("scancode queue not initialized");
        if let Some(s) = queue.pop() { return Poll::Ready(Some(s)); }
        WAKER.register(&cx.waker());
        match queue.pop() { Some(s) => { WAKER.take(); Poll::Ready(Some(s)) } None => Poll::Pending }
    }
}

pub async fn getline() -> alloc::string::String {
    use alloc::string::String;
    use alloc::vec::Vec;

    // Enable PS/2 keyboard port before starting the getline stream and
    // disable it before returning. This masks the keyboard at the controller
    // level (not the IRQ vector) so other input remains unaffected.
    fn read_status_port() -> u8 {
        use x86_64::instructions::port::Port;
        let mut p: Port<u8> = Port::new(0x64);
        unsafe { p.read() }
    }

    fn wait_input_clear_local(max_loops: usize) -> bool {
        for _ in 0..max_loops {
            if (read_status_port() & 0x02) == 0 { return true; }
        }
        false
    }

    fn write_controller_cmd_local(cmd: u8) -> bool {
        use x86_64::instructions::port::Port;
        if !wait_input_clear_local(10000) { return false; }
        let mut p: Port<u8> = Port::new(0x64);
        unsafe { p.write(cmd); }
        true
    }

    fn enable_keyboard_port() {
        // 0xAE = Enable first PS/2 port (keyboard)
        if !write_controller_cmd_local(0xAE) {
            crate::driver_framework::drivers::console::console_print_first("[kbd] Warning: failed to enable PS/2 keyboard port (0xAE)\n");
        } else {
            crate::driver_framework::drivers::console::console_print_first("[kbd] PS/2 keyboard port enabled\n");
        }
    }

    fn disable_keyboard_port() {
        // 0xAD = Disable first PS/2 port (keyboard)
        if !write_controller_cmd_local(0xAD) {
            crate::driver_framework::drivers::console::console_print_first("[kbd] Warning: failed to disable PS/2 keyboard port (0xAD)\n");
        } else {
            crate::driver_framework::drivers::console::console_print_first("[kbd] PS/2 keyboard port disabled\n");
        }
    }

    // Enable keyboard at controller before creating the stream so the device
    // will begin reporting scancodes. We'll disable it before returning.
    enable_keyboard_port();
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);

    let mut buf: Vec<char> = Vec::new();

    while let Some(sc) = scancodes.next().await {
        if let Ok(Some(ev)) = keyboard.add_byte(sc) {
            if let Some(key) = keyboard.process_keyevent(ev) {
                match key {
                    DecodedKey::Unicode(character) => {
                        match character {
                            '\n' | '\r' => {
                                // echo newline and return
                                println!("");
                                let s: String = buf.iter().collect();
                                // disable keyboard before returning
                                disable_keyboard_port();
                                return s;
                            }
                            '\x08' => {
                                // backspace - remove last char if any
                                if let Some(_) = buf.pop() {
                                    // Move cursor back, overwrite with space, move back again
                                    // Many VGA terminals don't interpret backspace, so emulate
                                    print!("\x08 \x08");
                                }
                            }
                            c => {
                                buf.push(c);
                                print!("{}", c);
                            }
                        }
                    }
                    DecodedKey::RawKey(_key) => {
                        // ignore raw keys for line input
                    }
                }
            }
        }
    }

    // If the stream ended, disable keyboard and return whatever we have
    disable_keyboard_port();
    buf.iter().collect()
}

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(), layouts::Us104Key, HandleControl::Ignore);
    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => print!("{}", character),
                    DecodedKey::RawKey(k) => print!("{:?}", k),
                }
            }
        }
    }
}
