/// Like the `print!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! print {
    ($($arg:tt)*) => ($crate::vga_buffer::_print(format_args!($($arg)*)));
}

/// Like the `println!` macro in the standard library, but prints to the VGA text buffer.
#[macro_export]
macro_rules! println {
    () => ($crate::print!("\n"));
    ($($arg:tt)*) => ($crate::print!("{}\n", format_args!($($arg)*)));
}

#[macro_export]
macro_rules! setcolor {
    ($fg:expr, $bg:expr) => {
        $crate::vga_buffer::WRITER.lock().set_color($fg, $bg);
    };
}

#[macro_export]
macro_rules! setpos {
    ($row:expr, $col:expr) => {
        $crate::vga_buffer::WRITER.lock().set_position($row, $col);
    };
}

#[macro_export]
macro_rules! cls {
    () => {
        $crate::vga_buffer::WRITER.lock().clear_screen();
    };
}