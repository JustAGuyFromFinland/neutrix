use crate::*;
use x86_64::structures::idt::*;
use spin;
use lazy_static::lazy_static;
use pc_keyboard::*;
use conquer_once::spin::OnceCell;
use crossbeam_queue::ArrayQueue;
use core::{pin::Pin, task::{Poll, Context}};
use futures_util::stream::Stream;
use futures_util::task::AtomicWaker;
use futures_util::StreamExt;

static SCANCODE_QUEUE: OnceCell<ArrayQueue<u8>> = OnceCell::uninit();

pub extern "x86-interrupt" fn keyboard_interrupt_handler(
    _stack_frame: InterruptStackFrame
) {
    use x86_64::instructions::port::Port;

    let mut port = Port::new(0x60);
    let scancode: u8 = unsafe { port.read() };
    add_scancode(scancode); // new

    unsafe {
        PICS.lock()
            .notify_end_of_interrupt(InterruptIndex::Keyboard.as_u8());
    }
}

pub(crate) fn add_scancode(scancode: u8) {
    if let Ok(queue) = SCANCODE_QUEUE.try_get() {
        if let Err(_) = queue.push(scancode) {
            println!("WARNING: scancode queue full; dropping keyboard input");
        } else {
            WAKER.wake(); // new
        }
    } else {
        println!("WARNING: scancode queue uninitialized");
    }
}

pub struct ScancodeStream {
    _private: (),
}

impl ScancodeStream {
    pub fn new() -> Self {
        SCANCODE_QUEUE.try_init_once(|| ArrayQueue::new(100))
            .expect("ScancodeStream::new should only be called once");
        ScancodeStream { _private: () }
    }
}

impl Stream for ScancodeStream {
    type Item = u8;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context) -> Poll<Option<u8>> {
        let queue = SCANCODE_QUEUE
            .try_get()
            .expect("scancode queue not initialized");

        // fast path
        if let Some(scancode) = queue.pop() {
            return Poll::Ready(Some(scancode));
        }

        WAKER.register(&cx.waker());
        match queue.pop() {
            Some(scancode) => {
                WAKER.take();
                Poll::Ready(Some(scancode))
            }
            None => Poll::Pending,
        }
    }
}

static WAKER: AtomicWaker = AtomicWaker::new();

pub async fn print_keypresses() {
    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(),
        layouts::Us104Key, HandleControl::Ignore);

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
                match key {
                    DecodedKey::Unicode(character) => print!("{}", character),
                    DecodedKey::RawKey(key) => print!("{:?}", key),
                }
            }
        }
    }
}

/// Read a line (until Enter) from the keyboard asynchronously.
///
/// Handles backspace by removing the last character from the buffer and
/// echoing the appropriate backspace behavior. Returns the typed line
/// (without the terminating newline).
///
/// Usage:
/// ```ignore
/// // inside an async task on the kernel executor
/// let line: alloc::string::String = devices::ps2keyboard::getline().await;
/// println!("received line: {}", line);
/// ```
///
/// Note: `getline` is async and must be awaited from the kernel's async executor
/// (see `Executor::spawn` / `Task::new` usage in `src/main.rs`).
pub async fn getline() -> alloc::string::String {
    use alloc::string::String;
    use alloc::vec::Vec;

    let mut scancodes = ScancodeStream::new();
    let mut keyboard = Keyboard::new(ScancodeSet1::new(),
        layouts::Us104Key, HandleControl::Ignore);

    let mut buf: Vec<char> = Vec::new();

    while let Some(scancode) = scancodes.next().await {
        if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
            if let Some(key) = keyboard.process_keyevent(key_event) {
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