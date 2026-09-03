ENTRY(_start)

SECTIONS
{
    . = 0x40080000;

    .text : {
        *(.text._start)
        *(.text.vectors)
        *(.text .text.*)
    }

    .rodata : {
        *(.rodata .rodata.*)
    }

    .data : {
        *(.data .data.*)
    }

    .bss : {
        *(.bss .bss.*)
        *(.bss.stack)
    }

    /DISCARD/ : {
        *(.eh_frame)
        *(.comment)
    }
}
