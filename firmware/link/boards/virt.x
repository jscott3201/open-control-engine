/* The riscv32imac-unknown-none-elf image's layout: QEMU's riscv32 `virt` machine, started with
   `-bios none`, whose reset code jumps to the start of DRAM at 0x80000000. riscv-rt places its
   entry first in REGION_TEXT, so the image's code starts there; its data follows in the next
   256 KiB. `sifive_e`, the board rusty-bacnet emulates, has 16 KiB of RAM, too little for the
   allocator arena an engine fixture decodes into (see src/bin/blob_frames.rs), and `virt` has an
   RV32IMAC hart with the same semihosting. The size check builds and measures the same image QEMU
   runs. riscv-rt's link.x includes this file before its own SECTIONS. */
MEMORY
{
  FLASH : ORIGIN = 0x80000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x80100000, LENGTH = 256K
}

/* riscv-rt places its sections through these aliases. */
REGION_ALIAS("REGION_TEXT", FLASH);
REGION_ALIAS("REGION_RODATA", FLASH);
REGION_ALIAS("REGION_DATA", RAM);
REGION_ALIAS("REGION_BSS", RAM);
REGION_ALIAS("REGION_HEAP", RAM);
REGION_ALIAS("REGION_STACK", RAM);

/* Unwind tables. riscv-rt's link.x does not place .eh_frame, so the linker would put it in flash.
   Only unwinding reads it, and the image never unwinds: the `firmware` profile builds it with
   panic = "abort". cortex-m-rt drops ARM's unwind tables (.ARM.exidx) the same way. */
SECTIONS
{
  /DISCARD/ : { *(.eh_frame) *(.eh_frame_hdr) }
}
