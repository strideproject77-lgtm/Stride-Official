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

- 2026-11-07: Added bounded structural instruction-stream decoding for each non-empty method and checked incoming register layout against method prototypes. The check accounts for instance receivers and wide (`long`/`double`) incoming parameters. The partial operand-bounds pass covers 12x, 11x/11n, 22x, 32x, selected 21x, 22t/22c/22s, 23x, and 22b formats. Follow-ups add explicit register-list validation for 35c/45cc and contiguous register-window validation for 3rc/4rcc, plus two-register bounds checks for selected wide moves, constants, array/field operations, conversions, and long/double arithmetic. Try/catch parsing validates ordered non-overlapping try ranges, handler-list structure, encoded handler offsets, catch target addresses, and instruction-boundary alignment for protected-range starts/ends and catch targets. Class-data parsing now rejects direct/virtual method category mismatches and duplicate method definitions within a class. These are still partial structural checks: instruction-boundary checks for try/catch addresses, invoke argument-width checks against resolved prototypes, and other verifier rules remain pending. This is not full Dalvik verification, class loading, or execution. The invoke, wide-pair, and try/catch changes are committed but have not yet been tested in CI.

- 2026-11-07: Added the first metadata-backed `ClassRegistry` in `runtime-rust/crates/dex-runtime`. It atomically registers class definitions from DEX inventories, rejects duplicate descriptors, and resolves declared methods by exact owner/name/prototype or by a DEX `method_id` index. Tests cover successful app-method lookup, exact-prototype mismatch, invalid indices, duplicate registration, and expected unresolved Android framework references when no boot class path is registered. This is not bytecode invocation, inheritance/virtual dispatch, field linking, or Android framework support. These new Rust tests have not been run in CI.

- 2026-11-07: Added resolved DEX `method_id` and `field_id` inventories to `ClassInventory`. Each reference now exposes its declaring class descriptor, member name, and prototype/type, including references to framework/library classes not defined in the current DEX. Added a method-reference fixture assertion. This is identifier resolution groundwork for class loading and invoke/field linking; it does not execute methods, resolve runtime classes, or prove HCR compatibility. The change has not been run through CI.

- 2026-11-07 follow-up: Damaged/corrupt DEX entry extraction is now counted as skipped instead of aborting inspection of the whole APK.
- Added Swift unit coverage with a minimal valid DEX fixture for the Rust FFI class/method counts, plus malformed and empty-buffer rejection cases. These new Swift tests have **not** been executed yet.
- PR workflow run #89 passed the Rust tests for commit `e36c804`; the iOS/IPA job was skipped. Subsequent invoke-register, wide-pair, and try/catch validation changes have not yet been tested in CI. Pushes no longer trigger CI; Rust tests run on PRs or a deliberate manual dispatch. Manual dispatch defaults to Rust-only; the iOS/IPA job requires explicitly enabling the `build_ios` input.

- 2026-11-07: Connected `APKArchiveInspector` to the Swift `StrideDexInventory` Rust FFI during APK import. It now reports analyzed DEX count plus class/method totals, and the package detail UI shows those counts. Decompression and FFI input are bounded to 64 MiB per DEX, 128 MiB total declared DEX data, and 32 DEX entries; malformed/oversized DEX files are counted as skipped rather than treated as executable.
- This Swift/ZIPFoundation integration has not yet been compiled or run. No manual macOS workflow run was available through the connected GitHub tools in this session. The next verification must compile the app and exercise APK import with a valid DEX fixture and malformed/oversized inputs; do not claim the Swift bridge is verified until then.

- Added bounds-checked DEX class/method inventory parsing at runtime-rust/crates/dex-inspector/src/classes.rs. It reads class definitions, encoded fields and methods, descriptors, names, prototypes, and referenced code-item boundaries. Non-empty method bodies now also pass instruction-stream structural decoding. This is not full Dalvik verification or execution.
- Added unit tests for valid inventory, foreign class ownership, and invalid file size. These passed in Rust workflow run #19.
- Improved the SwiftUI APK library/detail experience in Sources/StrideApp/ContentView.swift. It shows APK and DEX counts, recent packages, details, ABI chips, and explicit runtime limitations. The screen has not yet been visually checked in Xcode.
- CI runs Rust tests on PRs and deliberate manual dispatches; ordinary pushes do not trigger workflows. Manual dispatch defaults to Rust-only. The macOS device build/unsigned IPA runs only when `build_ios` is explicitly enabled. Simulator tests remain omitted because the hosted runner could not connect to its Simulator runtime.
- Added runtime-rust/crates/dex-ffi as a workspace crate. It exports a C ABI that accepts one DEX byte buffer and returns class/method counts with explicit error codes. Added the C header at runtime-rust/crates/dex-ffi/include/stride_dex_ffi.h. FFI tests cover null pointers and malformed DEX input.
- Built the FFI for iOS device and simulator, packaged an XCFramework, linked it with XcodeGen, and added a Swift wrapper at Sources/StrideApp/Runtime/StrideDexInventory.swift. APKArchiveInspector now calls that wrapper during APK import.
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

- Latest unverified integration: Swift import calls the Rust DEX inventory FFI and surfaces class/method counts. Validate compilation and bounded extraction before relying on these results. Latest Rust changes add partial incoming-register, invoke-register, wide-pair, and try/catch structural checks; their new tests are pending.

- The DEX inventory and FFI crate passed Rust CI in runs #38, #43, and #51; device/simulator Rust compilation and Swift linkage passed in run #51.
- APKArchiveInspector now calls the DEX FFI during bounded APK import and surfaces class/method counts. The new Swift integration and added Swift fixture tests remain unverified until the next deliberate iOS build/test checkpoint. The FFI inspects metadata; it does not execute code.
- The UI compiled in the successful iOS 15 physical-device build in run #51, but has not been visually checked in Simulator or on an iPhone.
- The interpreter supports only a small primitive opcode subset; it is not ART.
- Native image planning does not map or execute ELF libraries.
- No real-device APK launch test exists.
- Simulator test execution is unverified because the hosted Mac runner could not access its Simulator service.

## First compatibility target: Hill Climb Racing (original)

- Chosen target: the original Hill Climb Racing by Fingersoft, package `com.fingersoft.hillclimb`; explicitly not Hill Climb Racing 2 or 3. Official listing: https://play.google.com/store/apps/details?id=com.fingersoft.hillclimb
- Selection rationale: among Hill Climb Racing, Dr. Driving, and Subway Surfers, the original Hill Climb Racing's side-view physics gameplay is the smallest likely rendering scope. This is an engineering inference, not a claim that its runtime is simple. Its official listing describes physics-based driving and offline play.
- Latest public build located on 2026-11-07: **v1.73.2 (build 1016)**, uploaded 2026-10-29. Package: `com.fingersoft.hillclimb`. APKMirror lists three bundle variants; the broad ARM64 + ARMv7 variant is Android 7.0+ (API 24), 160–640 dpi, about 114.50 MB. Another ARM64-only high-DPI variant is Android 10+ (API 29). Public app metadata reports target SDK Android 16 (API 36). Sources: APKMirror release page and variant listing; AppDashboard metadata. These are third-party catalog observations, not a substitute for inspecting the exact downloaded bundle.
- Target baseline selected for research: **v1.73.2 (1016), ARM64 + ARMv7, Android 7.0+ variant**. This is a version/variant selection, not yet a pinned binary: no APK/XAPK has been downloaded and hashed or parsed in this workspace. Record the exact bundle SHA-256, manifest, DEX files, native libraries, resources and startup dependencies before target-specific implementation or compatibility claims.
- Current Hill Climb launch compatibility: **0% verified**. Public metadata research and DEX structural decoding are groundwork only; no HCR APK has been parsed and no Android app execution exists.
- Rough planning estimate: **6–18+ months of full-time solo work** for a serious attempt at the current full game, with no guarantee. This is an inference from the missing execution core/framework/native/graphics layers; exact binary inspection may move the estimate. It is not a delivery promise. A small proof-of-launch app comes first and will take substantially less work than the full game.

### Target-specific progress gates

Show progress only when a gate has evidence; avoid percentage changes for code volume alone:

- 0%: target version/variant selected from public metadata, but exact APK bundle not pinned or parsed; no launch path.
- 10%: exact APK frozen and manifest, DEX, ABI, resources, and startup dependencies documented.
- 25%: DEX verification/class loading and the target's startup method path work.
- 50%: Activity lifecycle and initial game screen render on device.
- 75%: vehicle controls, terrain rendering, physics loop, and fail/restart flow work.
- 90%: assets/audio and offline single-player path pass; known unsupported APIs documented.
- 100%: defined end-to-end acceptance checklist passes on a named iPhone/iOS version for the pinned APK.

This percentage describes only the pinned Hill Climb build, never general Android compatibility. Report it in status updates and keep this handoff current after each completed gate.

### Workflow policy

- Batch implementation changes. Do not trigger workflows for routine commits; use PR checks or deliberate manual Rust-only checks periodically. Enable `build_ios` only at a real Swift/linkage integration gate or when an IPA is explicitly needed.
- macOS/XCFramework/IPA runs are manual-only via workflow_dispatch; the `build_ios` input defaults to false.
- Do not merge PR #1 until checks and review pass.


## Next steps

1. Continue implementation without routine workflow runs. At a deliberate verification checkpoint, run Rust-only tests first. Enable `build_ios` only when Swift/Xcode linkage needs verification or an IPA is explicitly needed.
2. Verify the Rust ABI on valid DEX input and exercise malformed/oversized DEX handling. Do not use a real HCR APK as a substitute for a controlled fixture.
3. Obtain the selected v1.73.2 (1016) ARM64 + ARMv7 Android 7+ bundle; record its SHA-256 and inspect manifest, DEX, native libraries/resources, then write a requirement report. Public metadata alone is insufficient for that gate.
4. Build a tiny clean-room launch fixture to measure verifier, class-loader, Activity, rendering, and input work before promising HCR dates.
5. Expand toward the pinned HCR build by implementing observed dependencies; update the target percentage only when acceptance evidence passes.

## Resume checklist

- Confirm branch feat/dex-class-method-loader and current workflow status.
- Read the Hill Climb target gate, current 0% status, and workflow policy above.
- Recent code step: APK import now invokes the Rust DEX inventory FFI with bounded extraction; compilation and behavior remain unverified.
- Latest implementation: class inventory decodes method instruction streams, validates incoming register-word layout, checks selected register formats including invoke/range lists/windows and selected wide pairs, structurally validates try/catch ranges/handlers and instruction-boundary alignment, checks direct/virtual method classification plus duplicate definitions, and now resolves method_id/field_id entries to declaring-class descriptors, names, prototypes, and field types. This is still static metadata; next build the minimum runtime class/method resolution path against a controlled fixture, then use the exact pinned HCR bundle to determine which framework/native dependencies are actually needed. The invoke/wide/try-catch batch and new `ClassRegistry` tests need a Rust-only CI checkpoint after a deliberate pause; never enable `build_ios` for this check. The registry currently resolves only exact declared methods and deliberately leaves Android framework references unresolved; next add parsed field definitions and exact field lookup, then a controlled method-link/invoke fixture. Review the new Swift tests at a later iOS integration checkpoint. Obtain/pin the exact official APK before writing HCR-specific app-execution code.


## Latest CI failure diagnosis (2026-11-07)

Workflow run #86 (`42368614689`) failed in the Rust tests: 18 passed and 2 failed. Both failures came from an incorrectly encoded `const/4` test fixture, not from the register validator: DEX format 11n stores register A in bits 8–11, while the original fixture `0x1012` put `1` in the literal nibble (bits 12–15), leaving the register as `v0`. Corrected both fixtures to `0x0112` in `lib.rs` and `classes.rs`. This correction has not yet been rerun in CI; do not mark tests green until a new run passes.
