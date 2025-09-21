use core::arch::asm;

pub unsafe fn outb(port: u16, val: u8) {
    asm!(
        "out dx, al",
        in("dx") port,
        in("al") val,
    );
}

pub unsafe fn inb(port: u16) -> u8 {
    let mut ret: u8;
    asm!(
        "in al, dx",
        in("dx") port,
        out("al") ret,
    );
    ret
}

pub unsafe fn outw(port: u16, val: u16) {
    asm!(
        "out dx, ax",
        in("dx") port,
        in("ax") val,
    );
}

pub unsafe fn inw(port: u16) -> u16 {
    let mut ret: u16;
    asm!(
        "in ax, dx",
        in("dx") port,
        out("ax") ret,
    );
    ret
}

pub unsafe fn outdw(port: u16, val: u32) {
    asm!(
        "out dx, eax",
        in("dx") port,
        in("eax") val,
    );
}

pub unsafe fn indw(port: u16) -> u32 {
    let mut ret: u32;
    asm!(
        "in eax, dx",
        in("dx") port,
        out("eax") ret,
    );
    ret
}