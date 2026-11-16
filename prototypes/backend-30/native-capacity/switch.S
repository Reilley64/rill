/* Only stack bootstrap/return; all admission and ABI mechanisms use LLVM23. */
#ifdef __APPLE__
#define NAME(x) _##x
#else
#define NAME(x) x
#endif
.text
.globl NAME(switch_stack)
.p2align 4
NAME(switch_stack):
#if defined(__aarch64__)
  stp x19, x30, [sp, #-16]!
  mov x19, sp
  mov sp, x0
  mov x0, x2
  blr x1
  mov sp, x19
  ldp x19, x30, [sp], #16
  ret
#elif defined(__x86_64__)
  pushq %r12
  movq %rsp, %r12
  movq %rdi, %rsp
  movq %rdx, %rdi
  callq *%rsi
  movq %r12, %rsp
  popq %r12
  retq
#else
#error unsupported
#endif
#if defined(__linux__)
.section .note.GNU-stack,"",@progbits
#endif
