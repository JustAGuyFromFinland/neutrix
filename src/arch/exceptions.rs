use crate::*;
use x86_64::structures::idt::*;

pub extern "x86-interrupt" fn division_by_zero(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: DIVISION BY ZERO\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn debug_error(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: DEBUG\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn overflow(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: OVERFLOW\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn bre(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn invalid_opcode(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: INVALID OPCODE\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn dno(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: DEVICE NOT AVAILABLE\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn breakpoint(
    stack_frame: InterruptStackFrame)
{
    println!("EXCEPTION: BREAKPOINT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn double_fault(
    stack_frame: InterruptStackFrame, _error_code: u64) -> !
{
    panic!("EXCEPTION: DOUBLE FAULT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn invalid_tss(
    stack_frame: InterruptStackFrame, _error_code: u64)
{
    panic!("EXCEPTION: INVALID TSS\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn snp(
    stack_frame: InterruptStackFrame, _error_code: u64)
{
    panic!("EXCEPTION: SEGMENT NOT PRESENT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn ssf(
    stack_frame: InterruptStackFrame, _error_code: u64)
{
    panic!("EXCEPTION: STACK SEGMENT FAULT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn gpf(
    stack_frame: InterruptStackFrame, _error_code: u64)
{
    panic!("EXCEPTION: GENERAL PROTECTION FAULT\n{:#?}", stack_frame);
}

pub extern "x86-interrupt" fn pf(
    stack_frame: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    use x86_64::registers::control::Cr2;

    println!("EXCEPTION: PAGE FAULT");
    println!("Accessed Address: {:?}", Cr2::read());
    println!("Error Code: {:?}", error_code);
    println!("{:#?}", stack_frame);
    hlt();
}