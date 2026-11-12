# AArch64 guest interpreter foundation

The `dex-runtime::arm64_guest` module adds a bounded, interpreter-based AArch64
execution primitive. It is deliberately separate from the ELF loader and JNI
registry until those components can share a validated guest address-space and
calling-convention contract.

## Implemented instruction subset

- NOP
- MOVZ and MOVK (32-bit and 64-bit forms)
- ADD/SUB immediate, including NZCV flag-setting forms
- B, BL and B.cond
- CBZ and CBNZ
- BR, BLR and RET
- Unsigned-offset integer LDR/STR for 32-bit and 64-bit values

Unknown or unsupported encodings fail closed with the instruction word. Guest
loads and stores are bounds-checked against Stride-owned memory. An instruction
budget limits interpreter work per call.

## Explicit limitations

This is not a complete ARMv8-A implementation. It does not yet implement SIMD/FP,
system registers, exceptions/interrupts, atomics, exclusive accesses, TLS,
syscalls, signal delivery, dynamic code generation, or Android ABI call stubs.
The current memory object is a bounded contiguous region, not a process virtual
memory manager with ELF segment permissions. Branch targets are alignment-checked
but are not yet checked against executable page permissions.

It is not wired to the ELF native linker, Bionic symbol trampolines, JNI native
registration, `JNI_OnLoad`, NativeActivity, or Cocos2d-x. Therefore this commit
does not make Hill Climb Racing launchable and must not be described as an HCR
loading-screen milestone.

## Validation status

Unit tests have been added for basic MOVZ/ADD/RET execution, instruction-budget
enforcement, invalid guest memory access, and unknown instruction rejection.
They have intentionally not been run in this implementation-only batch.
