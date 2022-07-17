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

    la      t0, boot_pagetable
    srli    t0, t0, 12
    li      t1, 8 << 60
    or      t0, t0, t1
    csrw    satp, t0
    sfence.vma
# 使用页表后pc还是在物理地址上，所以需要物理地址直接映射一部分
    la t0,far_jmp_point
    li t1, 0xffffffd800000000
    add t0,t0,t1
    jr t0

# 在此之后使用的是direct mapping之后的虚拟地址
far_jmp_point:
# far jmp以后需要刷新icache
    fence.i

# 屏蔽所有中断
	csrw sie, zero
	csrw sip, zero

# 重置寄存器
    call reset_regs

# 清理bss，stack在bss中
    la t0, sbss_clear
    la t1, ebss_clear
    ble t1, t0, clear_bss_done
clear_bss:
    # store double words = 8 bytes
    sd zero, 0(t0)
    add t0, t0, 8
    blt t0, t1, clear_bss
clear_bss_done:

# 设置栈
# todo：为栈添加magic number用于检测溢出
    add t0, a0, 1
    slli t0, t0, 14
    la sp, boot_stack
    add sp, sp, t0

# 跳转到kernel执行
    call start_kernel

loop:
    j loop


reset_regs:

	li	sp, 0
	li	gp, 0
	li	tp, 0
	li	t0, 0
	li	t1, 0
	li	t2, 0
	li	s0, 0
	li	s1, 0
	li	a2, 0
	li	a3, 0
	li	a4, 0
	li	a5, 0
	li	a6, 0
	li	a7, 0
	li	s2, 0
	li	s3, 0
	li	s4, 0
	li	s5, 0
	li	s6, 0
	li	s7, 0
	li	s8, 0
	li	s9, 0
	li	s10, 0
	li	s11, 0
	li	t3, 0
	li	t4, 0
	li	t5, 0
	li	t6, 0
	csrw	sscratch, 0

#ifdef CONFIG_FPU
#	csrr	t0, CSR_MISA
#	andi	t0, t0, (COMPAT_HWCAP_ISA_F | COMPAT_HWCAP_ISA_D)
#	beqz	t0, .Lreset_regs_done
#
#	li	t1, SR_FS
#	csrs	CSR_STATUS, t1
#	fmv.s.x	f0, zero
#	fmv.s.x	f1, zero
#	fmv.s.x	f2, zero
#	fmv.s.x	f3, zero
#	fmv.s.x	f4, zero
#	fmv.s.x	f5, zero
#	fmv.s.x	f6, zero
#	fmv.s.x	f7, zero
#	fmv.s.x	f8, zero
#	fmv.s.x	f9, zero
#	fmv.s.x	f10, zero
#	fmv.s.x	f11, zero
#	fmv.s.x	f12, zero
#	fmv.s.x	f13, zero
#	fmv.s.x	f14, zero
#	fmv.s.x	f15, zero
#	fmv.s.x	f16, zero
#	fmv.s.x	f17, zero
#	fmv.s.x	f18, zero
#	fmv.s.x	f19, zero
#	fmv.s.x	f20, zero
#	fmv.s.x	f22, zero
#	fmv.s.x	f23, zero
#	fmv.s.x	f24, zero
#	fmv.s.x	f26, zero
#	fmv.s.x	f27, zero
#	fmv.s.x	f28, zero
#	fmv.s.x	f29, zero
#	fmv.s.x	f30, zero
#	fmv.s.x	f31, zero
#	csrw	fcsr, 0
	/* note that the caller must clear SR_FS */
#endif /* CONFIG_FPU */
.Lreset_regs_done:
	ret

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
    .global boot_pagetable
boot_pagetable:
    .quad 0
    .quad 0
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 351
    .quad (0x80000 << 10) | 0xcf # VRWXAD
    .zero 8 * 157
