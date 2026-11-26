use std::ffi::c_void;

const USABLE_BYTES: usize = 1024 * 1024;

pub fn run_on_entry_stack(entry: extern "C" fn()) -> Result<(), &'static [u8]> {
    // SAFETY: `sysconf` only reads a system constant.
    let page = usize::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })
        .ok()
        .filter(|page| page.is_power_of_two())
        .ok_or(&b"cannot read the page size"[..])?;
    let usable = USABLE_BYTES.next_multiple_of(page);
    let mapped = usable + page;
    // SAFETY: an anonymous private mapping of `mapped` bytes at a kernel-chosen address.
    let base = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            mapped,
            libc::PROT_READ | libc::PROT_WRITE,
            libc::MAP_PRIVATE | libc::MAP_ANON,
            -1,
            0,
        )
    };
    if base == libc::MAP_FAILED {
        return Err(b"cannot reserve the entry stack");
    }
    // SAFETY: the first page of the mapping just created is guard space, below the usable bytes.
    if unsafe { libc::mprotect(base, page, libc::PROT_NONE) } != 0 {
        unmap(base, mapped);
        return Err(b"cannot protect the entry stack's guard page");
    }
    // SAFETY: `mapped` is the length of the mapping, so the result is its one-past-the-end
    // address.
    let top = unsafe { base.cast::<u8>().add(mapped) };
    #[cfg(feature = "checking")]
    crate::stack_region::publish(top.addr() - usable, top.addr());
    // SAFETY: `top` is the page-aligned end of the mapping, so it is 16-byte aligned, and
    // the `usable` bytes below it are writable.
    unsafe { switch_and_call(top, entry) };
    #[cfg(feature = "checking")]
    crate::stack_region::publish(0, 0);
    unmap(base, mapped);
    Ok(())
}

fn unmap(base: *mut c_void, length: usize) {
    // SAFETY: `base` and `length` are the whole mapping, which nothing uses any more.
    unsafe { libc::munmap(base, length) };
}

#[cfg(target_arch = "x86_64")]
unsafe fn switch_and_call(top: *mut u8, entry: extern "C" fn()) {
    // SAFETY: `r12` is callee-saved, so it still holds the old stack pointer after the call.
    unsafe {
        std::arch::asm!(
            "mov r12, rsp",
            "mov rsp, {top}",
            "call {entry}",
            "mov rsp, r12",
            top = in(reg) top,
            entry = in(reg) entry,
            out("r12") _,
            clobber_abi("C"),
        );
    }
}

#[cfg(target_arch = "aarch64")]
unsafe fn switch_and_call(top: *mut u8, entry: extern "C" fn()) {
    // SAFETY: `x20` is callee-saved, so it still holds the old stack pointer after the call.
    unsafe {
        std::arch::asm!(
            "mov x20, sp",
            "mov sp, {top}",
            "blr {entry}",
            "mov sp, x20",
            top = in(reg) top,
            entry = in(reg) entry,
            out("x20") _,
            clobber_abi("C"),
        );
    }
}
