.section .text
.global trap_entry
.global kern_trap_ret
.global user_trap_ret
.type trap_entry, @function
.align 2

trap_entry:
  csrrw sp, sscratch ,sp
  bnez sp , trap_user

  csrrw sp, sscratch ,sp

trap_kern:
    addi sp, sp, -35*8
    sd x1, 1*8(sp)
    sd x2, 2*8(sp)
    sd x3, 3*8(sp)
    sd x4, 4*8(sp)
    sd x5, 5*8(sp)
    sd x6, 6*8(sp)
    sd x7, 7*8(sp)
    sd x8, 8*8(sp)
    sd x9, 9*8(sp)
    sd x10, 10*8(sp)
    sd x11, 11*8(sp)
    sd x12, 12*8(sp)
    sd x13, 13*8(sp)
    sd x14, 14*8(sp)
    sd x15, 15*8(sp)
    sd x16, 16*8(sp)
    sd x17, 17*8(sp)
    sd x18, 18*8(sp)
    sd x19, 19*8(sp)
    sd x20, 20*8(sp)
    sd x21, 21*8(sp)
    sd x22, 22*8(sp)
    sd x23, 23*8(sp)
    sd x24, 24*8(sp)
    sd x25, 25*8(sp)
    sd x26, 26*8(sp)
    sd x27, 27*8(sp)
    sd x28, 28*8(sp)
    sd x29, 29*8(sp)
    sd x30, 30*8(sp)
    sd x31, 31*8(sp)

    #store sepc
    csrr a0, sepc
    sd a0 , 0(sp)

    #store scause
    csrr a0, scause
    sd a0, 32*8(sp)

    #store sscratch
    csrr a0, sscratch
    sd a0, 33*8(sp)

    #store sstatus
    csrr a0, sstatus
    sd a0, 34*8(sp)

    # set sscratch to 0
    csrw sscratch, x0

    mv a0, sp
    csrr a1, scause

    bgez a1, .kern_exc_handler
  .kern_irq_handler:
    jal irq_handler
    j kern_trap_ret
  .kern_exc_handler:
    jal exc_handler

  kern_trap_ret:

    #restore sstatus
    ld a0, 34*8(sp)
    csrw sstatus, a0

    #restore sscratch
    ld a0, 33*8(sp)
    csrw sscratch, a0

    #restore scause
    ld a0, 32*8(sp)
    csrw scause, a0

    #restore sepc
    ld a0, 0(sp)
    csrw sepc ,a0

    ld x1, 1*8(sp)
    ld x2, 2*8(sp)
    ld x3, 3*8(sp)
    ld x4, 4*8(sp)
    ld x5, 5*8(sp)
    ld x6, 6*8(sp)
    ld x7, 7*8(sp)
    ld x8, 8*8(sp)
    ld x9, 9*8(sp)
    ld x10, 10*8(sp)
    ld x11, 11*8(sp)
    ld x12, 12*8(sp)
    ld x13, 13*8(sp)
    ld x14, 14*8(sp)
    ld x15, 15*8(sp)
    ld x16, 16*8(sp)
    ld x17, 17*8(sp)
    ld x18, 18*8(sp)
    ld x19, 19*8(sp)
    ld x20, 20*8(sp)
    ld x21, 21*8(sp)
    ld x22, 22*8(sp)
    ld x23, 23*8(sp)
    ld x24, 24*8(sp)
    ld x25, 25*8(sp)
    ld x26, 26*8(sp)
    ld x27, 27*8(sp)
    ld x28, 28*8(sp)
    ld x29, 29*8(sp)
    ld x30, 30*8(sp)
    ld x31, 31*8(sp)

    addi sp, sp, 35*8
    sret

trap_user:
  addi sp, sp, -35*8
  sd x1, 1*8(sp)
  sd x2, 2*8(sp)
  sd x3, 3*8(sp)
  sd x4, 4*8(sp)
  sd x5, 5*8(sp)
  sd x6, 6*8(sp)
  sd x7, 7*8(sp)
  sd x8, 8*8(sp)
  sd x9, 9*8(sp)
  sd x10, 10*8(sp)
  sd x11, 11*8(sp)
  sd x12, 12*8(sp)
  sd x13, 13*8(sp)
  sd x14, 14*8(sp)
  sd x15, 15*8(sp)
  sd x16, 16*8(sp)
  sd x17, 17*8(sp)
  sd x18, 18*8(sp)
  sd x19, 19*8(sp)
  sd x20, 20*8(sp)
  sd x21, 21*8(sp)
  sd x22, 22*8(sp)
  sd x23, 23*8(sp)
  sd x24, 24*8(sp)
  sd x25, 25*8(sp)
  sd x26, 26*8(sp)
  sd x27, 27*8(sp)
  sd x28, 28*8(sp)
  sd x29, 29*8(sp)
  sd x30, 30*8(sp)
  sd x31, 31*8(sp)

  #store sepc
  csrr a0, sepc
  sd a0 , 0(sp)

  #store scause
  csrr a0, scause
  sd a0, 32*8(sp)

  #store sscratch
  csrr a0, sscratch
  sd a0, 33*8(sp)

  #store sstatus
  csrr a0, sstatus
  sd a0, 34*8(sp)

# set sscratch to 0
  csrw sscratch, x0

  mv a0, sp
  csrr a1, scause
  bgez a1, .user_exc_handler
.user_irq_handler:
  jal irq_handler
  j user_trap_ret
.user_exc_handler:
  jal exc_handler
user_trap_ret:
  #restore sstatus
  ld a0, 34*8(sp)
  csrw sstatus, a0

  #restore sscratch
  ld a0, 33*8(sp)
  csrw sscratch, a0

  #restore scause
  ld a0, 32*8(sp)
  csrw scause, a0

  #restore sepc
  ld a0, 0(sp)
  csrw sepc ,a0

  ld x1, 1*8(sp)
  ld x2, 2*8(sp)
  ld x3, 3*8(sp)
  ld x4, 4*8(sp)
  ld x5, 5*8(sp)
  ld x6, 6*8(sp)
  ld x7, 7*8(sp)
  ld x8, 8*8(sp)
  ld x9, 9*8(sp)
  ld x10, 10*8(sp)
  ld x11, 11*8(sp)
  ld x12, 12*8(sp)
  ld x13, 13*8(sp)
  ld x14, 14*8(sp)
  ld x15, 15*8(sp)
  ld x16, 16*8(sp)
  ld x17, 17*8(sp)
  ld x18, 18*8(sp)
  ld x19, 19*8(sp)
  ld x20, 20*8(sp)
  ld x21, 21*8(sp)
  ld x22, 22*8(sp)
  ld x23, 23*8(sp)
  ld x24, 24*8(sp)
  ld x25, 25*8(sp)
  ld x26, 26*8(sp)
  ld x27, 27*8(sp)
  ld x28, 28*8(sp)
  ld x29, 29*8(sp)
  ld x30, 30*8(sp)
  ld x31, 31*8(sp)

  addi sp, sp, 35*8
  csrrw sp, sscratch ,sp
  sret
