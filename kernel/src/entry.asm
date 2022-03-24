    .section .text.init
    .globl _start
_start:
    # a0 == hartid
    # pc == 0x80200000
    # sp == 0x800xxxxx
    # 未展开页表之前pc指向物理地址 所以一定不能使用auipc之类的指令
    # la包含auipc,不能随意使用

    # set boot pagetable
    # satp = (8 << 60) | PPN(boot_page_table_sv39)
    li a0, 'A'
    li x17, 1
    ecall

    la      t0, boot_pagetable
    srli    t0, t0, 12
    li      t1, 8 << 60
    or      t0, t0, t1
    csrw    satp, t0
    sfence.vma
# 使用页表后pc还是在物理地址上，所以需要物理地址直接映射一部分
    li a0, 'B'
    li x17, 1
    ecall

    la t0,far_jmp_point
    li t1, 0xffffffd800000000
    add t0,t0,t1
    jr t0

far_jmp_point:
    auipc a1,0
    add t0, a0, 1
    slli t0, t0, 14
    la sp, boot_stack
    add sp, sp, t0
    call start_kernel

loop:
    j loop

    .section .bss.stack
    .align 12
boot_stack:
    .space 4096 * 4 * 2
    .globl boot_stack_top
boot_stack_top:

#static DIRECT_MAP_START:usize = 0xffffffd800000000;
#static DIRECT_MAP_END:usize = 0xfffffff700000000;
# 101 1000 10 -> 354项
# 0xffffffd880000000 -> 0x80000000
    .section .data
    .align 12
boot_pagetable:
    .quad 0
    .quad 0
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 351
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 157
