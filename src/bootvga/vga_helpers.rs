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
        // Try to update VBE framebuffer console colors as well (best-effort).
        {
            use crate::driver_framework::drivers::{vbe_vga, console};
            let fg32 = vbe_vga::vbe_color_from_vga_color($fg);
            let bg32 = vbe_vga::vbe_color_from_vga_color($bg);
            console::console_set_colors_first(fg32, bg32);
        }
        $crate::vga_buffer::WRITER.lock().set_color($fg, $bg);
    };
}

#[macro_export]
macro_rules! setpos {
    ($row:expr, $col:expr) => {
        // Update VBE console cursor (best-effort)
        {
            use crate::driver_framework::drivers::console;
            console::console_set_cursor_first($col, $row);
        }
        $crate::vga_buffer::WRITER.lock().set_position($row, $col);
    };
}

#[macro_export]
macro_rules! cls {
    () => {
        {
            use crate::driver_framework::drivers::console;
            console::console_clear_first();
        }
        $crate::vga_buffer::WRITER.lock().clear_screen();
    };
}