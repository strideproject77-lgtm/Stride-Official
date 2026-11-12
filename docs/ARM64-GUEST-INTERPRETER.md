# AArch64 guest execution and ELF bridge

Stride now has a bounded AArch64 interpreter and an ELF-backed guest image
materializer in `dex-runtime::arm64_guest` and
`dex-runtime::native_execution`.

## Instruction coverage in this implementation batch

- NOP
- MOVZ and MOVK (32-bit and 64-bit forms)
- ADD/SUB immediate and shifted-register forms
- AND/ORR/EOR/ANDS shifted-register forms
- ADR and ADRP
- B, BL and B.cond
- CBZ and CBNZ
- BR, BLR and RET
- LDR literal and LDRSW literal
- Unsigned-offset integer LDR/STR (32-bit and 64-bit)
- Integer LDP/STP pair forms (offset, pre-index and post-index)


- Logical-immediate AND/ORR/EOR/ANDS forms (including MOV/TST aliases)
- Conditional select family: CSEL, CSINC, CSINV and CSNEG
- Added SBFM/BFM/UBFM bitfield instructions and sign-extending LDRSB/LDRSH/LDRSW unsigned-offset forms.
- Unsigned-offset byte/halfword/word/doubleword LDR/STR
- Unscaled-offset LDUR/STUR byte/halfword/word/doubleword

Native image integration also preserves DT_INIT/DT_INIT_ARRAY metadata and can
run the discovered constructors after relocations. The caller must initialize
dependencies first and invoke this once per module in dependency order.

Unknown or unsupported encodings fail closed with the instruction word. The
interpreter has a hard per-call instruction budget. Guest loads, stores and
instruction fetches are checked against mapped page permissions when strict
protection mode is enabled.

## ELF image integration

`NativeGuestImage::from_elf` parses an ELF64 little-endian AArch64 image,
plans PT_LOAD pages, materializes file and BSS bytes, applies supported
transactional relocations through a caller-provided guest-symbol resolver,
maps ELF page permissions, and allocates a bounded non-executable guest stack.

`NativeGuestImage::from_linked_module` accepts a module produced by the
existing `NativeLinker`. The caller must first run
`NativeLinker::apply_relocations` and check its returned diagnostics. This
path preserves the linker's dependency-aware symbol resolution and exported
symbol table.

The image exposes exported-symbol lookup, JNI short/long symbol candidate resolution, AAPCS64
integer/pointer argument setup (x0-x7), and bounded guest execution for exported
functions. It also parses bounded guest JNINativeMethod arrays for
RegisterNatives-style dispatch, and provides JNI_OnLoad and native entry
invocation helpers when the caller supplies guest JNI handles.

## Explicit limitations

This is not a complete ARMv8-A implementation. SIMD/FP instructions, system
registers, exceptions/interrupts, atomics, exclusive accesses, TLS, syscalls,
signal delivery, dynamic code generation, stack-passed ABI arguments, FP/SIMD
AAPCS64 arguments, and host-to-guest JNI object/reference conversion are not
implemented. The guest stack is a private bounded region; this is not a complete
Android process virtual-memory manager or thread scheduler.

The JNI helpers only prepare guest ABI arguments, parse guest native-method
metadata, and execute the resolved guest symbol. They are not yet wired into a
real JavaVM/JNIEnv function table. A complete JavaVM/JNIEnv object model,
RegisterNatives callback interception, reference conversion and callback bridge
still need to be implemented before ordinary Android JNI code can be considered
compatible.
NativeActivity and Cocos2d-x initialization, EGL/OpenGL ES rendering, and HCR
asset/API integration are not complete. Therefore this work does not establish
that Hill Climb Racing reaches its original loading screen.

## Validation status

Regression tests have been added for the supported execution primitives,
permission enforcement, budget enforcement, ELF input rejection, AAPCS64
argument bounds, and JNI symbol mangling. They have intentionally not been run
in this implementation-only batch.
