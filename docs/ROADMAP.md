# Runtime roadmap

The milestones below are implementation gates, not compatibility claims.

## Foundation
- [x] Native SwiftUI application shell
- [x] APK archive import and safe local storage
- [x] APK ZIP structure inspection
- [x] Explicit runtime capability reporting
- [x] Safe ELF identification primitives in Rust
- [x] CI build and Rust unit-test validation

## Package and executable analysis
- [x] DEX header and map-list bounds inspection
- [x] DEX code_item boundary reader (instruction decoding still pending)
- [x] Android binary XML chunk and string-pool structural decoder
- [x] ELF program-header, PT_LOAD and PT_DYNAMIC range inspection
- [x] Resolve manifest strings into package metadata, SDK level, permissions and components
- [x] DEX opcode-width decoder and bounded branch/payload target checks (full verifier pending)
- [x] ELF dynamic tags, SysV/GNU hash symbol bounds, dynamic symbols and REL/RELA relocation records (loading/applying pending)

## Runtime implementation
- [ ] Android process and memory model
- [ ] ARM32 / ARM64 instruction execution or translation strategy
- [ ] Bionic-compatible libc and linker boundary
- [ ] JNI object, method and native registration bridge
- [ ] DEX interpreter and runtime services
- [ ] Android framework compatibility services
- [ ] Graphics, input, audio, and lifecycle integration

## Device validation
- [ ] Run a purpose-built native Android test APK end-to-end
- [ ] Validate process isolation, memory limits, and shutdown
- [ ] Publish per-ABI and per-API compatibility results backed by device logs

No app is considered runnable until the complete execution path works on a physical iOS device.
