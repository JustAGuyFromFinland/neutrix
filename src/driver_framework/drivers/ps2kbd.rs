use crate::*;
use alloc::boxed::Box;
use core::pin::Pin;
use core::task::Poll;
use futures_util::stream::Stream;
use futures_util::task::AtomicWaker;
use futures_util::StreamExt;
use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use pc_keyboard::*;
use x86_64::structures::idt::InterruptStackFrame;

use crate::driver_framework::driver::Driver;
use crate::driver_framework::device::{DeviceInfo, Resource, ResourceKind};

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();
static WAKER: AtomicWaker = AtomicWaker::new();

pub struct Ps2KbdDriver;

impl Ps2KbdDriver {
    pub fn new() -> Self { Ps2KbdDriver }

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
                crate::arch::idt::register_irq_handler(vector, Ps2KbdDriver::irq_handler);
            }
        }
        Ok(())
    }

    fn stop(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        // no-op: resources freed on release
    }

    fn release(&self, _device: &crate::driver_framework::device::DeviceHandle) {
        // nothing for now
    }
}

pub fn boxed_driver() -> Box<dyn Driver> {
    Box::new(Ps2KbdDriver::new())
}

// Provide a small async stream API for consumers (getline/print_keypresses) to use
pub struct ScancodeStream { _private: () }
impl ScancodeStream {
    pub fn new() -> Self { SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100)).ok(); ScancodeStream { _private: () } }
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

    // If the stream ended, return whatever we have
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
