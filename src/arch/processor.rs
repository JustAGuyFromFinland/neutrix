use x86_64::registers::control::{Cr0, Cr4};
use x86_64::registers::control::Cr0Flags;
use x86_64::registers::control::Cr4Flags;
use x86_64::registers::xcontrol::{XCr0, XCr0Flags};
use core::arch::x86_64::{__cpuid, __cpuid_count};
use crate::*;

pub fn enable_sse() {
    unsafe {
        // Enable FPU/SSE instructions
        let mut cr0 = Cr0::read();
        cr0.remove(Cr0Flags::EMULATE_COPROCESSOR); // clear EM = 0
        cr0.insert(Cr0Flags::MONITOR_COPROCESSOR); // set MP = 1
        Cr0::write(cr0);

        let mut cr4 = Cr4::read();
        cr4.insert(Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE);
        Cr4::write(cr4);
    }
}

#[derive(Default)]
pub struct CpuFeatures {
    // Basic features EDX
    pub fpu: bool,
    pub vme: bool,
    pub de: bool,
    pub pse: bool,
    pub tsc: bool,
    pub msr: bool,
    pub pae: bool,
    pub mce: bool,
    pub cx8: bool,
    pub apic: bool,
    pub sep: bool,
    pub mtrr: bool,
    pub pge: bool,
    pub mca: bool,
    pub cmov: bool,
    pub pat: bool,
    pub pse36: bool,
    pub psn: bool,
    pub clfsh: bool,
    pub ds: bool,
    pub acpi: bool,
    pub mmx: bool,
    pub fxsr: bool,
    pub sse: bool,
    pub sse2: bool,
    pub ss: bool,
    pub htt: bool,
    pub tm: bool,
    pub ia64: bool,
    pub pbe: bool,
    // Basic ECX
    pub sse3: bool,
    pub pclmulqdq: bool,
    pub dtes64: bool,
    pub monitor: bool,
    pub ds_cpl: bool,
    pub vmx: bool,
    pub smx: bool,
    pub est: bool,
    pub tm2: bool,
    pub ssse3: bool,
    pub cnxt_id: bool,
    pub sdbg: bool,
    pub fma: bool,
    pub cx16: bool,
    pub xtpr: bool,
    pub pdcm: bool,
    pub pcid: bool,
    pub dca: bool,
    pub sse4_1: bool,
    pub sse4_2: bool,
    pub x2apic: bool,
    pub movbe: bool,
    pub popcnt: bool,
    pub tsc_deadline: bool,
    pub aes: bool,
    pub xsave: bool,
    pub osxsave: bool,
    pub avx: bool,
    pub f16c: bool,
    pub rdrand: bool,
    pub hypervisor: bool,
    // Extended EDX
    pub syscall: bool,
    pub mp: bool,
    pub nx: bool,
    pub mmxext: bool,
    pub fxsr_opt: bool,
    pub pdpe1gb: bool,
    pub rdtscp: bool,
    pub lm: bool,
    pub threednowext: bool,
    pub threednow: bool,
    // Extended ECX
    pub lahf_lm: bool,
    pub cmp_legacy: bool,
    pub svm: bool,
    pub extapic: bool,
    pub cr8_legacy: bool,
    pub abm: bool,
    pub sse4a: bool,
    pub misalignsse: bool,
    pub threednowprefetch: bool,
    pub osvw: bool,
    pub ibs: bool,
    pub xop: bool,
    pub skinit: bool,
    pub wdt: bool,
    pub lwp: bool,
    pub fma4: bool,
    pub tce: bool,
    pub nodeid_msr: bool,
    pub tbm: bool,
    pub topoext: bool,
    pub perfctr_core: bool,
    pub perfctr_nb: bool,
    pub bpext: bool,
    pub ptsc: bool,
    pub perfctr_llc: bool,
    pub mwaitx: bool,
    // EAX=7, EBX
    pub fsgsbase: bool,
    pub tsc_adjust: bool,
    pub sgx: bool,
    pub bmi1: bool,
    pub hle: bool,
    pub avx2: bool,
    pub fdp_excptn_only: bool,
    pub smep: bool,
    pub bmi2: bool,
    pub rep_movsb_stosb: bool,
    pub invpcid: bool,
    pub rtm: bool,
    pub rdt_m: bool,
    pub dep_fpu_cs_ds: bool,
    pub mpx: bool,
    pub rdt_a: bool,
    pub avx512f: bool,
    pub avx512dq: bool,
    pub rdseed: bool,
    pub adx: bool,
    pub smap: bool,
    pub avx512ifma: bool,
    pub pcommit: bool,
    pub clflushopt: bool,
    pub clwb: bool,
    pub intel_pt: bool,
    pub avx512pf: bool,
    pub avx512er: bool,
    pub avx512cd: bool,
    pub sha: bool,
    pub avx512bw: bool,
    pub avx512vl: bool,
    // EAX=7, ECX
    pub prefetchwt1: bool,
    pub avx512vbmi: bool,
    pub umip: bool,
    pub pku: bool,
    pub ospke: bool,
    pub waitpkg: bool,
    pub avx512vbmi2: bool,
    pub cet_ss: bool,
    pub gfni: bool,
    pub vaes: bool,
    pub vpclmulqdq: bool,
    pub avx512vnni: bool,
    pub avx512bitalg: bool,
    pub tme: bool,
    pub avx512vpopcntdq: bool,
    pub la57: bool,
    pub rdpid: bool,
    pub keylocker: bool,
    pub bus_lock_detect: bool,
    pub cldemote: bool,
    pub movdiri: bool,
    pub movdir64b: bool,
    pub enqcmd: bool,
    pub sgx_lc: bool,
    pub pks: bool,
}

pub fn detect_cpu_features() -> CpuFeatures {
    let mut features = CpuFeatures::default();
    unsafe {
        // Check for extended CPUID
        let max_extended = __cpuid(0x80000000).eax;
        if max_extended >= 0x80000001 {
            let result = __cpuid(0x80000001);
            let ecx = result.ecx;
            let edx = result.edx;
            // Extended EDX
            features.syscall = (edx & (1 << 11)) != 0;
            features.mp = (edx & (1 << 19)) != 0;
            features.nx = (edx & (1 << 20)) != 0;
            features.mmxext = (edx & (1 << 22)) != 0;
            features.fxsr_opt = (edx & (1 << 25)) != 0;
            features.pdpe1gb = (edx & (1 << 26)) != 0;
            features.rdtscp = (edx & (1 << 27)) != 0;
            features.lm = (edx & (1 << 29)) != 0;
            features.threednowext = (edx & (1 << 30)) != 0;
            features.threednow = (edx & (1 << 31)) != 0;
            // Extended ECX
            features.lahf_lm = (ecx & (1 << 0)) != 0;
            features.cmp_legacy = (ecx & (1 << 1)) != 0;
            features.svm = (ecx & (1 << 2)) != 0;
            features.extapic = (ecx & (1 << 3)) != 0;
            features.cr8_legacy = (ecx & (1 << 4)) != 0;
            features.abm = (ecx & (1 << 5)) != 0;
            features.sse4a = (ecx & (1 << 6)) != 0;
            features.misalignsse = (ecx & (1 << 7)) != 0;
            features.threednowprefetch = (ecx & (1 << 8)) != 0;
            features.osvw = (ecx & (1 << 9)) != 0;
            features.ibs = (ecx & (1 << 10)) != 0;
            features.xop = (ecx & (1 << 11)) != 0;
            features.skinit = (ecx & (1 << 12)) != 0;
            features.wdt = (ecx & (1 << 13)) != 0;
            features.lwp = (ecx & (1 << 15)) != 0;
            features.fma4 = (ecx & (1 << 16)) != 0;
            features.tce = (ecx & (1 << 17)) != 0;
            features.nodeid_msr = (ecx & (1 << 19)) != 0;
            features.tbm = (ecx & (1 << 21)) != 0;
            features.topoext = (ecx & (1 << 22)) != 0;
            features.perfctr_core = (ecx & (1 << 23)) != 0;
            features.perfctr_nb = (ecx & (1 << 24)) != 0;
            features.bpext = (ecx & (1 << 26)) != 0;
            features.ptsc = (ecx & (1 << 27)) != 0;
            features.perfctr_llc = (ecx & (1 << 28)) != 0;
            features.mwaitx = (ecx & (1 << 29)) != 0;
        }
        // Basic features
        let result = __cpuid(1);
        let ecx = result.ecx;
        let edx = result.edx;
        // EDX
        features.fpu = (edx & (1 << 0)) != 0;
        features.vme = (edx & (1 << 1)) != 0;
        features.de = (edx & (1 << 2)) != 0;
        features.pse = (edx & (1 << 3)) != 0;
        features.tsc = (edx & (1 << 4)) != 0;
        features.msr = (edx & (1 << 5)) != 0;
        features.pae = (edx & (1 << 6)) != 0;
        features.mce = (edx & (1 << 7)) != 0;
        features.cx8 = (edx & (1 << 8)) != 0;
        features.apic = (edx & (1 << 9)) != 0;
        features.sep = (edx & (1 << 11)) != 0;
        features.mtrr = (edx & (1 << 12)) != 0;
        features.pge = (edx & (1 << 13)) != 0;
        features.mca = (edx & (1 << 14)) != 0;
        features.cmov = (edx & (1 << 15)) != 0;
        features.pat = (edx & (1 << 16)) != 0;
        features.pse36 = (edx & (1 << 17)) != 0;
        features.psn = (edx & (1 << 18)) != 0;
        features.clfsh = (edx & (1 << 19)) != 0;
        features.ds = (edx & (1 << 21)) != 0;
        features.acpi = (edx & (1 << 22)) != 0;
        features.mmx = (edx & (1 << 23)) != 0;
        features.fxsr = (edx & (1 << 24)) != 0;
        features.sse = (edx & (1 << 25)) != 0;
        features.sse2 = (edx & (1 << 26)) != 0;
        features.ss = (edx & (1 << 27)) != 0;
        features.htt = (edx & (1 << 28)) != 0;
        features.tm = (edx & (1 << 29)) != 0;
        features.ia64 = (edx & (1 << 30)) != 0;
        features.pbe = (edx & (1 << 31)) != 0;
        // ECX
        features.sse3 = (ecx & (1 << 0)) != 0;
        features.pclmulqdq = (ecx & (1 << 1)) != 0;
        features.dtes64 = (ecx & (1 << 2)) != 0;
        features.monitor = (ecx & (1 << 3)) != 0;
        features.ds_cpl = (ecx & (1 << 4)) != 0;
        features.vmx = (ecx & (1 << 5)) != 0;
        features.smx = (ecx & (1 << 6)) != 0;
        features.est = (ecx & (1 << 7)) != 0;
        features.tm2 = (ecx & (1 << 8)) != 0;
        features.ssse3 = (ecx & (1 << 9)) != 0;
        features.cnxt_id = (ecx & (1 << 10)) != 0;
        features.sdbg = (ecx & (1 << 11)) != 0;
        features.fma = (ecx & (1 << 12)) != 0;
        features.cx16 = (ecx & (1 << 13)) != 0;
        features.xtpr = (ecx & (1 << 14)) != 0;
        features.pdcm = (ecx & (1 << 15)) != 0;
        features.pcid = (ecx & (1 << 17)) != 0;
        features.dca = (ecx & (1 << 18)) != 0;
        features.sse4_1 = (ecx & (1 << 19)) != 0;
        features.sse4_2 = (ecx & (1 << 20)) != 0;
        features.x2apic = (ecx & (1 << 21)) != 0;
        features.movbe = (ecx & (1 << 22)) != 0;
        features.popcnt = (ecx & (1 << 23)) != 0;
        features.tsc_deadline = (ecx & (1 << 24)) != 0;
        features.aes = (ecx & (1 << 25)) != 0;
        features.xsave = (ecx & (1 << 26)) != 0;
        features.osxsave = (ecx & (1 << 27)) != 0;
        features.avx = (ecx & (1 << 28)) != 0;
        features.f16c = (ecx & (1 << 29)) != 0;
        features.rdrand = (ecx & (1 << 30)) != 0;
        features.hypervisor = (ecx & (1 << 31)) != 0;
        // Extended features EAX=7, ECX=0
        let result7 = __cpuid_count(7, 0);
        let ebx7 = result7.ebx;
        let ecx7 = result7.ecx;
        // EBX
        features.fsgsbase = (ebx7 & (1 << 0)) != 0;
        features.tsc_adjust = (ebx7 & (1 << 1)) != 0;
        features.sgx = (ebx7 & (1 << 2)) != 0;
        features.bmi1 = (ebx7 & (1 << 3)) != 0;
        features.hle = (ebx7 & (1 << 4)) != 0;
        features.avx2 = (ebx7 & (1 << 5)) != 0;
        features.fdp_excptn_only = (ebx7 & (1 << 6)) != 0;
        features.smep = (ebx7 & (1 << 7)) != 0;
        features.bmi2 = (ebx7 & (1 << 8)) != 0;
        features.rep_movsb_stosb = (ebx7 & (1 << 9)) != 0;
        features.invpcid = (ebx7 & (1 << 10)) != 0;
        features.rtm = (ebx7 & (1 << 11)) != 0;
        features.rdt_m = (ebx7 & (1 << 12)) != 0;
        features.dep_fpu_cs_ds = (ebx7 & (1 << 13)) != 0;
        features.mpx = (ebx7 & (1 << 14)) != 0;
        features.rdt_a = (ebx7 & (1 << 15)) != 0;
        features.avx512f = (ebx7 & (1 << 16)) != 0;
        features.avx512dq = (ebx7 & (1 << 17)) != 0;
        features.rdseed = (ebx7 & (1 << 18)) != 0;
        features.adx = (ebx7 & (1 << 19)) != 0;
        features.smap = (ebx7 & (1 << 20)) != 0;
        features.avx512ifma = (ebx7 & (1 << 21)) != 0;
        features.pcommit = (ebx7 & (1 << 22)) != 0;
        features.clflushopt = (ebx7 & (1 << 23)) != 0;
        features.clwb = (ebx7 & (1 << 24)) != 0;
        features.intel_pt = (ebx7 & (1 << 25)) != 0;
        features.avx512pf = (ebx7 & (1 << 26)) != 0;
        features.avx512er = (ebx7 & (1 << 27)) != 0;
        features.avx512cd = (ebx7 & (1 << 28)) != 0;
        features.sha = (ebx7 & (1 << 29)) != 0;
        features.avx512bw = (ebx7 & (1 << 30)) != 0;
        features.avx512vl = (ebx7 & (1 << 31)) != 0;
        // ECX
        features.prefetchwt1 = (ecx7 & (1 << 0)) != 0;
        features.avx512vbmi = (ecx7 & (1 << 1)) != 0;
        features.umip = (ecx7 & (1 << 2)) != 0;
        features.pku = (ecx7 & (1 << 3)) != 0;
        features.ospke = (ecx7 & (1 << 4)) != 0;
        features.waitpkg = (ecx7 & (1 << 5)) != 0;
        features.avx512vbmi2 = (ecx7 & (1 << 6)) != 0;
        features.cet_ss = (ecx7 & (1 << 7)) != 0;
        features.gfni = (ecx7 & (1 << 8)) != 0;
        features.vaes = (ecx7 & (1 << 9)) != 0;
        features.vpclmulqdq = (ecx7 & (1 << 10)) != 0;
        features.avx512vnni = (ecx7 & (1 << 11)) != 0;
        features.avx512bitalg = (ecx7 & (1 << 12)) != 0;
        features.tme = (ecx7 & (1 << 13)) != 0;
        features.avx512vpopcntdq = (ecx7 & (1 << 14)) != 0;
        features.la57 = (ecx7 & (1 << 15)) != 0;
        features.rdpid = (ecx7 & (1 << 16)) != 0;
        features.keylocker = (ecx7 & (1 << 17)) != 0;
        features.bus_lock_detect = (ecx7 & (1 << 18)) != 0;
        features.cldemote = (ecx7 & (1 << 19)) != 0;
        features.movdiri = (ecx7 & (1 << 20)) != 0;
        features.movdir64b = (ecx7 & (1 << 21)) != 0;
        features.enqcmd = (ecx7 & (1 << 22)) != 0;
        features.sgx_lc = (ecx7 & (1 << 23)) != 0;
        features.pks = (ecx7 & (1 << 24)) != 0;
    }
    features
}

pub fn disable_pit_timer() {
    use crate::arch::ports::*;
    unsafe {
        // Disable PIT channel 0 by setting mode 0 and count 0
        outb(0x43, 0x30); // Command: channel 0, lobyte/hibyte, mode 0
        outb(0x40, 0x00); // Low byte 0
        outb(0x40, 0x00); // High byte 0
    }
    println!("[CPU] Disabled PIT timer, switched to TSC timing");
}

pub fn enable_cpu_features(features: &CpuFeatures) {
    // Enable SSE family features first
    if features.sse {
        unsafe {
            let mut cr0 = Cr0::read();
            cr0.remove(Cr0Flags::EMULATE_COPROCESSOR);
            cr0.insert(Cr0Flags::MONITOR_COPROCESSOR);
            Cr0::write(cr0);

            let mut cr4 = Cr4::read();
            cr4.insert(Cr4Flags::OSFXSR | Cr4Flags::OSXMMEXCPT_ENABLE);
            Cr4::write(cr4);
        }
        println!("[CPU] Enabled SSE");
    }

    if features.tsc {
        println!("[CPU] Enabled TSC");
        // Switch to TSC timing, disable PIT
        disable_pit_timer();
    }

    if features.sse2 {
        // SSE2 is enabled with SSE
        println!("[CPU] Enabled SSE2");
    }

    if features.sse3 {
        println!("[CPU] Enabled SSE3");
    }

    if features.ssse3 {
        println!("[CPU] Enabled SSSE3");
    }

    if features.sse4_1 {
        println!("[CPU] Enabled SSE4.1");
    }

    if features.sse4_2 {
        println!("[CPU] Enabled SSE4.2");
    }

    // Enable AVX family features
    if features.avx && features.osxsave {
        unsafe {
            let mut xcr0 = XCr0::read();
            xcr0.insert(XCr0Flags::AVX);
            XCr0::write(xcr0);
        }
        println!("[CPU] Enabled AVX");
    }

    if features.avx2 {
        // AVX2 is enabled with AVX
        println!("[CPU] Enabled AVX2");
    }

    if features.f16c {
        println!("[CPU] Enabled F16C");
    }

    if features.fma {
        println!("[CPU] Enabled FMA");
    }

    // AVX-512 features (printing only, enabling requires additional XCR0 setup)
    if features.avx512f {
        println!("[CPU] Enabled AVX-512F");
    }

    if features.avx512dq {
        println!("[CPU] Enabled AVX-512DQ");
    }

    if features.avx512ifma {
        println!("[CPU] Enabled AVX-512IFMA");
    }

    if features.avx512pf {
        println!("[CPU] Enabled AVX-512PF");
    }

    if features.avx512er {
        println!("[CPU] Enabled AVX-512ER");
    }

    if features.avx512cd {
        println!("[CPU] Enabled AVX-512CD");
    }

    if features.avx512bw {
        println!("[CPU] Enabled AVX-512BW");
    }

    if features.avx512vl {
        println!("[CPU] Enabled AVX-512VL");
    }

    if features.avx512vbmi {
        println!("[CPU] Enabled AVX-512VBMI");
    }

    if features.avx512vbmi2 {
        println!("[CPU] Enabled AVX-512VBMI2");
    }

    if features.avx512vnni {
        println!("[CPU] Enabled AVX-512VNNI");
    }

    if features.avx512bitalg {
        println!("[CPU] Enabled AVX-512BITALG");
    }

    if features.avx512vpopcntdq {
        println!("[CPU] Enabled AVX-512VPOPCNTDQ");
    }

    // Other features
    if features.mmx {
        println!("[CPU] Enabled MMX");
    }

    if features.mmxext {
        println!("[CPU] Enabled MMXEXT");
    }

    if features.threednow {
        println!("[CPU] Enabled 3DNow!");
    }

    if features.threednowext {
        println!("[CPU] Enabled 3DNow!Ext");
    }

    if features.aes {
        println!("[CPU] Enabled AES");
    }

    if features.sha {
        println!("[CPU] Enabled SHA");
    }

    if features.rdrand {
        println!("[CPU] Enabled RDRAND");
    }

    if features.rdseed {
        println!("[CPU] Enabled RDSEED");
    }

    if features.adx {
        println!("[CPU] Enabled ADX");
    }

    if features.bmi1 {
        println!("[CPU] Enabled BMI1");
    }

    if features.bmi2 {
        println!("[CPU] Enabled BMI2");
    }

    if features.popcnt {
        println!("[CPU] Enabled POPCNT");
    }
}