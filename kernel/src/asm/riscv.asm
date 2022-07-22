.section .text
.global intr_disable
.type intr_disable, @function
.global intr_enable
.type intr_enable, @function
.align 2

intr_disable:
    csrrci a0, sstatus, 2
    ret
intr_enable:
    csrw sstatus, a0
    ret