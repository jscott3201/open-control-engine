//! The `blob-frames` image (issue #402): [`oce_firmware_link::run`] on bare metal, and the host
//! reference runner on a host.
//!
//! On `thumbv7em-none-eabihf` and `riscv32imac-unknown-none-elf` this is a `no_main` image. Its
//! reset entry hands the global allocator its arena, runs the embedded fixture through the `no_std`
//! engine entry, writes every line to semihosting stdout, then a `heap` line (the arena's use after
//! the run, which a host has no equivalent of), and exits through semihosting with the verdict's
//! code: 0 `PASS`, 1 `MISMATCH`, 3 `FAULT`, 101 on a panic. CI builds it and measures its size
//! against `firmware/link/size-budget.toml` on every engine change; `.github/workflows/qemu.yml`
//! runs it under QEMU nightly and diffs its lines against the host runner's. A CPU fault (a
//! HardFault on Cortex-M, a trap on RISC-V) lands in the runtime's default handler, which spins;
//! the QEMU runner's timeout reports it.
//!
//! On a host the bin runs the same [`oce_firmware_link::run`] and writes the same lines to stdout,
//! with the same exit code: the reference the emulated run must equal, line for line.
//!
//! The image has no `unsafe` code. The allocator is `linked_list_allocator`'s `LockedHeap`, a
//! linked-list first fit behind a spin lock on the targets' native atomics, so it needs no critical
//! section; its arena is a `static_cell::ConstStaticCell` that hands out its `&'static mut` once,
//! so giving the allocator its memory is the safe `init_from_slice`. The runtimes' entry macros
//! generate what the reset vector needs.

#![forbid(unsafe_code)]
#![cfg_attr(target_os = "none", no_std, no_main)]

/// The bare-metal image: allocator, entry, output and panic handler.
#[cfg(target_os = "none")]
mod image {
    use core::mem::MaybeUninit;

    use linked_list_allocator::LockedHeap;
    use semihosting::io::Write as _;
    use semihosting::process::exit;
    use static_cell::ConstStaticCell;

    use oce_firmware_link::{PANIC_EXIT_CODE, Sink, line::Line, run};

    /// Octets in the allocator's arena, a static in `.bss`. Measured on both targets under QEMU
    /// (the `firmware` profile, 2026-11-08, by rebuilding with smaller arenas): the run needs an
    /// arena of more than 30,336 and at most 30,464 octets (30,336 fails allocation inside
    /// `Executable::from_blob`; 30,464 passes), which is its peak including the first-fit
    /// allocator's fragmentation, and it still holds 29,344 after the last frame (the `heap`
    /// line). 64 KiB is about 2.15 times that peak, roughly 34 KiB of margin for a larger fixture;
    /// re-measure the same way when the fixture or the decoder changes.
    const ARENA: usize = 64 * 1024;

    /// The global allocator: empty until the entry hands it the arena.
    #[global_allocator]
    static HEAP: LockedHeap = LockedHeap::empty();

    /// The allocator's arena, handed out once. Its initializer is uninitialized bytes evaluated at
    /// compile time, so the arena lives in `.bss` and `take` hands out a reference to it without
    /// moving 64 KiB through the stack, in any profile.
    static MEMORY: ConstStaticCell<[MaybeUninit<u8>; ARENA]> =
        ConstStaticCell::new([MaybeUninit::uninit(); ARENA]);

    /// Write `bytes` to semihosting stdout; output that cannot be written is dropped, as the exit
    /// code still reports the verdict.
    fn write(bytes: &[u8]) {
        if let Ok(mut stdout) = semihosting::io::stdout() {
            let _ = stdout.write_all(bytes);
        }
    }

    /// The image's output: semihosting stdout, and the arena's use after the last frame.
    struct Semihosting {
        /// Octets of the arena in use after the last frame, while the executable is alive.
        heap_used: usize,
    }

    impl Sink for Semihosting {
        fn line(&mut self, bytes: &[u8]) {
            write(bytes);
        }

        fn after_frames(&mut self) {
            self.heap_used = HEAP.lock().used();
        }
    }

    /// Give the allocator its arena, run the fixture, report the heap and exit with the verdict.
    fn start() -> ! {
        // `take` returns the arena's `&'static mut` once (a second call panics).
        let arena = MEMORY.take();
        HEAP.lock().init_from_slice(arena);
        let mut sink = Semihosting { heap_used: 0 };
        let verdict = run(&mut sink);
        write(
            Line::new()
                .push_str("heap used ")
                .push_dec(sink.heap_used as u64)
                .push_str(" of ")
                .push_dec(ARENA as u64)
                .push_str(" octets after the last frame")
                .finish(),
        );
        exit(verdict.exit_code())
    }

    /// The reset entry on Cortex-M.
    #[cfg(target_arch = "arm")]
    #[cortex_m_rt::entry]
    fn main() -> ! {
        start()
    }

    /// The reset entry on RISC-V.
    #[cfg(target_arch = "riscv32")]
    #[riscv_rt::entry]
    fn main() -> ! {
        start()
    }

    /// Report a panic with its location and exit with [`PANIC_EXIT_CODE`].
    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo<'_>) -> ! {
        let mut text = Line::new();
        text.push_str("PANIC");
        if let Some(location) = info.location() {
            text.push_str(" at ")
                .push_str(location.file())
                .push_str(":")
                .push_dec(u64::from(location.line()));
        }
        write(text.finish());
        exit(PANIC_EXIT_CODE)
    }
}

/// The host reference runner: the same run, its lines on stdout, its verdict as the exit code.
#[cfg(not(target_os = "none"))]
fn main() {
    use std::io::Write as _;

    let mut stdout = std::io::stdout().lock();
    let mut failed = false;
    let verdict = oce_firmware_link::run(&mut |bytes: &[u8]| {
        failed |= stdout.write_all(bytes).is_err();
    });
    failed |= stdout.flush().is_err();
    std::process::exit(if failed { 1 } else { verdict.exit_code() });
}
