.section .text
.global switch_context
.type switch_context, @function
.align 2

switch_context:
#   void switch_context(context_t * cur , context_t * next);
#   a0 : cur
#   a1 : next

          sd ra, 0*8(a0)
          sd sp, 1*8(a0)
          sd s0, 2*8(a0)
          sd s1, 3*8(a0)
          sd s2, 4*8(a0)
          sd s3, 5*8(a0)
          sd s4, 6*8(a0)
          sd s5, 7*8(a0)
          sd s6, 8*8(a0)
          sd s7, 9*8(a0)
          sd s8, 10*8(a0)
          sd s9, 11*8(a0)
          sd s10, 12*8(a0)
          sd s11, 13*8(a0)

          csrr a3 , sscratch
          sd a3 , 14*8(a0)
          csrr a3 , sstatus
          sd a3 , 15*8(a0)

          ld ra, 0*8(a1)
          ld sp, 1*8(a1)
          ld s0, 2*8(a1)
          ld s1, 3*8(a1)
          ld s2, 4*8(a1)
          ld s3, 5*8(a1)
          ld s4, 6*8(a1)
          ld s5, 7*8(a1)
          ld s6, 8*8(a1)
          ld s7, 9*8(a1)
          ld s8, 10*8(a1)
          ld s9, 11*8(a1)
          ld s10, 12*8(a1)
          ld s11, 13*8(a1)

          ld a3, 14*8(a1)
          csrw sscratch, a3
          ld a3, 15*8(a1)
          csrw sstatus, a3

        ret