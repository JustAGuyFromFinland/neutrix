use x86_64::VirtAddr;
use x86_64::structures::tss::TaskStateSegment;
use lazy_static::lazy_static;
use core::convert::TryInto;
use x86_64::structures::gdt::*;
use x86_64::instructions::segmentation::*;
use x86_64::instructions::tables::*;

use crate::*;

pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

lazy_static! {
    static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            let stack_end = stack_start + STACK_SIZE.try_into().unwrap();
            stack_end
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();
        let kcode = gdt.append(Descriptor::kernel_code_segment());
		let kdata = gdt.append(Descriptor::kernel_data_segment());
		let ucode = gdt.append(Descriptor::user_code_segment());
		let udata = gdt.append(Descriptor::user_data_segment());
        let stss = gdt.append(Descriptor::tss_segment(&TSS));
        (gdt, Selectors {kcode, kdata, ucode, udata, stss})
    };
}

struct Selectors {
    kcode: SegmentSelector,
    kdata: SegmentSelector,
	ucode: SegmentSelector,
    udata: SegmentSelector,
	stss: SegmentSelector
}

pub fn init_gdt() {
    GDT.0.load();
	unsafe
	{
		CS::set_reg(GDT.1.kcode);
		DS::set_reg(GDT.1.kdata);
		ES::set_reg(GDT.1.kdata);
		FS::set_reg(GDT.1.kdata);
		GS::set_reg(GDT.1.kdata);
		SS::set_reg(GDT.1.kdata);
		load_tss(GDT.1.stss);
	}
}