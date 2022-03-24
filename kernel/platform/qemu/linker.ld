OUTPUT_ARCH(riscv64)
ENTRY(_start)

BASE_ADDR = 0xffffffd880200000;
SECTIONS
{
    . = BASE_ADDR;

    stext = .;
    .text.init : {
        *(.text.init)
    }

    .text : {
        *(.text .text.*)
    }
    etext = .;

    . = ALIGN(4K);
    srodata = .;
    .rodata : {
        *(.rodata .rodata.*)
        *(.srodata .srodata.*)
    }

    . = ALIGN(4K);
    erodata = .;
    sdata = .;
    .data : {
        *(.data .data.*)
        *(.sdata .sdata.*)
    }

    . = ALIGN(4K);
    edata = .;
    .bss : {
        *(.bss.stack)
        sbss_clear = .;
        *(.bss .bss.*)
        *(.sbss .sbss.*)
    }

    . = ALIGN(4K);
    ebss_clear = .;
    ekernel = .;

    /DISCARD/ : {
        *(.eh_frame)
    }
}