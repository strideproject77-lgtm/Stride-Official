# Runtime roadmap

The milestones below are implementation gates, not compatibility claims.

## Foundation
- [x] Native SwiftUI application shell
- [x] APK archive import and safe local storage
- [x] APK ZIP structure inspection
- [x] Explicit runtime capability reporting
- [x] Safe ELF identification primitives in Rust
- [ ] CI build and unit-test validation

## Package and executable analysis
- [ ] Android binary XML manifest decoder
- [ ] Package metadata, SDK level, permissions, components, and ABI inventory
- [ ] DEX header, map-list, and instruction validation
- [ ] ELF program-header, dynamic-section, relocation, and symbol analysis

## Runtime implementation
- [ ] Android process and memory model
- [ ] ARM32 / ARM64 instruction execution or translation strategy
- [ ] Bionic-compatible libc and linker boundary
- [ ] JNI object, method, and native registration bridge
- [ ] DEX interpreter and runtime services
- [ ] Android framework compatibility services
- [ ] Graphics, input, audio, and lifecycle integration

## Device validation
- [ ] Run a purpose-built native Android test APK end-to-end
- [ ] Validate process isolation, memory limits, and shutdown
- [ ] Publish per-ABI and per-API compatibility results backed by device logs

No app is considered runnable until the complete execution path works on a physical iOS device.
