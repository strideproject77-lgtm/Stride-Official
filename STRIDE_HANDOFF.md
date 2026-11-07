# Stride Handoff

Updated: 2026-11-07

## Goal

Build an iOS app that runs a growing, measured set of Android APKs, with a long-term goal of broad compatibility. Every support claim must name the APK/build, ABI, Android API assumptions, iPhone/iOS version, and real-device result. Importing or inspecting an APK does not mean it runs.

## Working state

- Repository: https://github.com/strideproject77-lgtm/Stride-Official
- Continue on branch: feat/dex-class-method-loader
- Draft PR: https://github.com/strideproject77-lgtm/Stride-Official/pull/1
- The branch was reconciled with latest main by a two-parent merge commit, preserving main's newer ELF address-overflow validation.
- Do not merge the draft PR yet. Read the latest workflow results and review the diff first.

## Completed

- Added bounds-checked DEX class/method inventory parsing at runtime-rust/crates/dex-inspector/src/classes.rs. It reads class definitions, encoded fields and methods, descriptors, names, prototypes, and referenced code-item boundaries. This is structural metadata parsing, not verification or execution.
- Added unit tests for valid inventory, foreign class ownership, and invalid file size. These passed in Rust workflow run #19.
- Improved the SwiftUI APK library/detail experience in Sources/StrideApp/ContentView.swift. It shows APK and DEX counts, recent packages, details, ABI chips, and explicit runtime limitations. The screen has not yet been visually checked in Xcode.
- Split CI so Ubuntu runs Rust checks for pull requests and pushes; the macOS job runs only on main and feat/dex-class-method-loader pushes and builds an unsigned device IPA. Simulator tests are currently omitted because the hosted runner could not connect to/download its Simulator runtime.
- Added runtime-rust/crates/dex-ffi as a workspace crate. It exports a C ABI that accepts one DEX byte buffer and returns class/method counts with explicit error codes. Added the C header at runtime-rust/crates/dex-ffi/include/stride_dex_ffi.h. FFI tests cover null pointers and malformed DEX input.
- The FFI is not yet linked into the iOS app; APKArchiveInspector still only inspects ZIP entries.
- No Husk source, code, tests, or assets were copied.

## Current boundary

The app imports APKs through ZIPFoundation and stores them under Documents/APKs. Rust has AXML/DEX/ELF inspection, a small integer-only DEX interpreter, and an ELF page planner. There is no Android execution backend, Bionic/JNI bridge, Android framework, or end-to-end APK launch yet.

## Product and licensing decisions

1. Continue Stride as an independent Swift/Rust implementation. Do not copy or port Husk source, comments, tests, assets, or distinctive implementation structure.
2. Use Android/DEX/ELF/JNI specifications, public documentation, and black-box behavior to define requirements. If source-level GPL study materially informs a distributable implementation, get a license review; reproducing implementation after reading GPL code is not a reliable way to avoid license obligations.
3. Treat “supports most apps” as a long-term objective. Measure compatibility by APK, ABI, API level, framework/service dependencies, graphics needs, and real-device results.
4. Report unsupported features clearly; do not label inspection as execution.

## CI status and workflow history

- PR run #19 passed the Ubuntu Rust tests after the DEX parser/native-loader fix.
- Runs #23 and #24 exposed unavailable simulator support on the hosted Mac. Workflow commit 913a9f3 removed simulator download/device-test steps; macOS CI now builds the physical-device target and packages an unsigned IPA.
- PR workflow run #38 passed the Ubuntu Rust tests, including the new FFI tests; its macOS job was skipped as designed for pull requests.
- Push run #37 passed Rust but the iOS compile failed because 11 SwiftUI .tracking modifiers require iOS 16 while the app targets iOS 15. Commit f4cd66a replaced them with iOS 15-compatible .kerning modifiers. Check the next push run to verify the device build and IPA artifact upload.
- PR #1 is draft and must remain unmerged until checks and review are complete.

## Known issues

- The DEX inventory and new FFI crate passed Rust CI in run #38.
- The DEX FFI only returns class/method counts; it is not linked into Swift and does not execute code.
- The UI has compiled in earlier CI but the current iOS 15 compatibility fix awaits the next macOS run; it has not been visually checked in Xcode.
- The interpreter supports only a small primitive opcode subset; it is not ART.
- Native image planning does not map or execute ELF libraries.
- No real-device APK launch test exists.
- Simulator test execution is unverified because the hosted Mac runner could not access its Simulator service.

## Next steps

1. Check the workflow triggered by commit f4cd66a. Confirm the iOS 15 compile succeeds and the unsigned IPA is uploaded; fix any remaining failures.
2. Add macOS CI steps that build the Rust static library for iOS device and simulator targets. Link the library into the generated Xcode project and verify the Swift app builds.
3. Add a Swift wrapper around stride_dex_inventory_counts. Extract classes*.dex from imported APKs and report class/method counts and parse errors in the package detail sheet.
4. Add valid DEX fixtures to exercise the C ABI through Swift before claiming the app uses Rust parsing.
5. Expand execution in measured stages: verifier and type model, objects/method calls/exceptions, ARM64 ELF loading, Bionic/JNI, then a small Android framework surface. Test each stage against a real APK cohort.

## Resume checklist

- Confirm current branch head and latest workflow run.
- Re-read this handoff, especially CI status and known issues.
- First check the push run after f4cd66a, then continue with macOS Rust target builds and Xcode linking.
