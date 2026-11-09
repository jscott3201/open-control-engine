/* The thumbv7em-none-eabihf image's layout: QEMU's `netduinoplus2` machine, an STM32F405
   (Cortex-M4F) with 1 MiB of flash at 0x08000000, which QEMU also maps at 0, where the core reads
   its vector table at reset, and 128 KiB of SRAM at 0x20000000. Its 64 KiB of core-coupled RAM at
   0x10000000 goes unused. The size check builds and measures the same image QEMU runs.
   cortex-m-rt's link.x includes this file before its own SECTIONS; it puts the stack at the top of
   RAM and turns the FPU on before main on an eabihf target. Adapted from rusty-bacnet's
   firmware/link/boards/netduinoplus2.x (same owner). */
MEMORY
{
  FLASH : ORIGIN = 0x08000000, LENGTH = 1024K
  RAM   : ORIGIN = 0x20000000, LENGTH = 128K
}
