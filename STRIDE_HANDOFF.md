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
- Built the FFI for iOS device and simulator, packaged an XCFramework, linked it with XcodeGen, and added a Swift wrapper at Sources/StrideApp/Runtime/StrideDexInventory.swift. APKArchiveInspector still does not call that wrapper.
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
- Push runs #37 and #41 exposed iOS 15 availability issues in SwiftUI letter spacing. Commits f4cd66a and 38906f4 fixed them. Push run #51 passed Rust, built the Rust XCFramework, linked and compiled the Swift wrapper, built the iOS device app, and uploaded an unsigned IPA artifact (562 KB; expires 2026-11-21): https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42361734733.
- PR #1 is draft and must remain unmerged until checks and review are complete.

## Known issues

- The DEX inventory and FFI crate passed Rust CI in runs #38, #43, and #51; device/simulator Rust compilation and Swift linkage passed in run #51.
- The DEX FFI returns class/method counts and is linked through a Swift wrapper, but APKArchiveInspector does not call it yet. It does not execute code.
- The UI compiled in the successful iOS 15 physical-device build in run #51, but has not been visually checked in Simulator or on an iPhone.
- The interpreter supports only a small primitive opcode subset; it is not ART.
- Native image planning does not map or execute ELF libraries.
- No real-device APK launch test exists.
- Simulator test execution is unverified because the hosted Mac runner could not access its Simulator service.

## Compatibility roadmap: Snake pilot, then broader execution

1. Start with the existing [Android Canvas Snake project](https://github.com/fpjunqueira/android-snake-game) as a candidate test app. Its repository describes it as Java and Canvas based and declares MIT; it has no published APK release, so build a debug APK before testing. Inspect its manifest, exact SDK levels, DEX version, permissions, APIs, and whether it uses View Canvas or SurfaceView/game-loop behavior. Do not assume “Canvas game” means no Android dependencies.
2. Freeze the tested APK identity: package name, version, SHA-256, min/target SDK, DEX files, and required Android APIs. If the candidate's requirements exceed the first vertical slice, build a tiny clean-room fixture APK for the smaller slice and keep Snake as the next target.
3. Implement only the measured slice required to launch Snake: DEX verification and typed values, then the object/method/exception behavior it actually exercises, Android activity lifecycle, and the specific drawing/input/timing APIs it uses. Keep runtime interfaces modular so later API and graphics implementations can expand without replacing the execution core.
4. Define “Snake supported” with a real-device acceptance flow: import, launch, draw the board, move the snake by touch, score, game over, and restart, all offline. Record unsupported calls and failures instead of silently faking behavior.
5. After Snake passes, choose at least two APKs whose inspected requirements overlap. Reuse the implemented DEX and Android API surface, then expand from observed gaps. Shared dependencies make compatibility more likely, but never guarantee it; test each APK/version.
6. Grow toward advanced execution in layers: broader DEX verifier/opcode and exception coverage; class loading, multidex/resources, and app lifecycle; more Android framework APIs and graphics; then ARM64 ELF loading, Bionic/JNI, and native libraries. Keep a compatibility matrix and real-device regression cohort at every stage.

This is the chosen delivery strategy: ship a small complete execution slice first, while keeping its runtime boundaries extensible toward wider compatibility. Do not begin by attempting to recreate full ART wholesale.

## Next steps

1. Run #51 built the current unsigned IPA successfully; retain artifact Stride-unsigned-IPA from that run. Do not merge PR #1 until review.
2. Connect APKArchiveInspector to StrideDexInventory.inspect. Stream each classes*.dex entry with a documented per-entry memory cap; show total class/method counts and parse failures in the package detail sheet.
3. Add a valid DEX fixture to Swift inspection tests and verify successful counts, malformed DEX handling, and oversized entry reporting.
5. Expand execution in measured stages: verifier and type model, objects/method calls/exceptions, ARM64 ELF loading, Bionic/JNI, then a small Android framework surface. Test each stage against a real APK cohort.

## Resume checklist

- Confirm branch feat/dex-class-method-loader and current workflow status.
- Read the compatibility roadmap above and choose the Snake fixture APK identity before implementing more execution behavior.
- First code task: wire APKArchiveInspector to the already-linked Rust DEX wrapper.
