# Stride Handoff

Updated: 2026-11-15 (Phase 50 Rust + physical iOS compile passed in workflow #914; device retest pending)

## Goal

Build an iOS app that runs a growing, measured set of Android APKs, with a long-term goal of broad compatibility. Every support claim must name the APK/build, ABI, Android API assumptions, iPhone/iOS version, and real-device result. Importing or inspecting an APK does not mean it runs.

## Working state

- Repository: https://github.com/strideproject77-lgtm/Stride-Official
- Continue on branch: feat/dex-class-method-loader
- Draft PR: https://github.com/strideproject77-lgtm/Stride-Official/pull/1
- The branch was reconciled with latest main by a two-parent merge commit, preserving main's newer ELF address-overflow validation.

## Common Compatibility Engine — current implementation checkpoint (2026-11-13)

- Phase 5 — Preserve DEX method access flags during hierarchy linking: **implementation complete**. The loader carries original DEX flags into `ClassHierarchy`; regressions cover abstract interface redeclarations plus private/static declarations through the loader path.
- Phase 6 — DEX invoke-kind compatibility: **implementation complete**. The interpreter rejects linked-app static/instance, direct/private/constructor, and class/interface invoke mismatches while leaving unresolved framework owners eligible for exact-signature JNI bridges. Focused regressions are present.
- UI extra delivered alongside this work: the Home welcome hero and app mark use a dark walnut/amber palette rather than the former purple/blue treatment.
- Validation state: Phases 11–15 passed the consolidated Rust workspace test job in workflow #701 (run ID `43481239721`, commit `a3727a3707925b1be608a9f85aa5ecc7b298e6ae`). The unsigned iOS IPA build job is queued/in progress; record its final result and artifact only after completion. IPA signing is not part of this workflow.
- Branch discipline: all changes remain on `feat/dex-class-method-loader`; `main` remains untouched.
- Do not merge the draft PR yet. Read the latest workflow results and review the diff first.

## Common Compatibility Engine — current implementation checkpoint (2026-11-14)

- Phase 26 — Activity task/backstack dispatch: **Completed**. Runtime-owned explicit Activity dispatch, Back restoration, guest DEX calls through JNI `Context.startActivity(Intent)`, `Activity.startActivityForResult`, and `Activity.finish()` pending-event routing are integrated and regression-tested.
- Phase 27 — One-Shot Diagnostic Report: **Completed and CI validated in workflow #757**. One TXT report combines package/split fingerprints, device environment, runtime metrics, launch-stage evidence, heuristic findings, host log and final Rust runtime snapshot. Real-device export review remains; never claim a guaranteed root cause.
- Phase 28 — Storage & Latest-Session Retention: **Implementation + CI build validated in workflow #764; device restoration review pending**.
  - Plain APK import now stores one canonical `base.apk`, rather than both `source.apk` and `base.apk`.
  - XAPK/APKS/APKM source containers are no longer retained as an extra full-size copy; a zero-byte format marker preserves reload metadata, and the base component is extracted directly to the canonical root path rather than extracted and copied again.
  - Starting a new launch clears previous session logs, reports, archives, and pre-launch global log noise; only the current session and its one-shot report remain.
  - Oversized rolling logs are truncated rather than moved into timestamped archive files.
  - Remaining: verify package restoration for APK/XAPK/APKS/APKM, ensure report export remains complete after log reset, and run the single consolidated Rust + iOS compile validation. Existing files from earlier app versions may remain until the next launch cleanup.
- UI phase requested by user — **Not started in this batch**: replace the current detail sheet with a dedicated app page, add last-played/app-size metadata and a single Diagnostics disclosure button, remove theme selection and image carousel, enforce pure-black surfaces, and add a proper About Stride screen. Do not mark this UI scope complete yet.
- Branch discipline: all changes remain on `feat/dex-class-method-loader`; `main` remains untouched. No IPA build/sign/export was run.

## Completed

- 2026-11-08: **End-to-End Compatibility & Regression Suite (compatibility_tests.rs)**:
  - Implemented end-to-end integration test validating the orchestration of:
    - Bionic libc/libm symbol resolution (sin, __android_log_print).
    - SharedPreferences multi-type persistence (put_string, put_int).
    - Canvas 2D geometry rasterization into Surface.
    - OpenGL ES context creation, clear commands, and presentation swap buffers.
    - MotionEvent input dispatch via InputQueue.
    - ServiceManager lifecycle (start_service, foreground promotion, stop_service).
    - PerformanceTelemetry rolling FPS and ANR watchdog verification.
- 2026-11-08: **Performance Telemetry, FPS Tracking, ANR Watchdog & Swift Metrics (	elemetry.rs, ExecutionBackend.swift, ContentView.swift)**:
  - Implemented PerformanceTelemetry with rolling 60-frame FPS estimation and 1% low frame-time calculation (one_percent_low_fps).
  - Implemented Android 5-second AnrWatchdog tracking main-thread dispatch duration and flagging application hangs.
  - Exposed stride_runtime_record_frame in C FFI, expanded StrideLaunchReport with verage_fps and heap_allocated_bytes.
  - Updated StrideArtExecutionBackend and ContentView.swift in the iOS app UI to report live FPS and managed heap RAM usage alongside lifecycle and opcode execution statistics.
- 2026-11-08: **ActivityManager Task Backstack, ServiceManager & XAPK Package Handler (ctivity_manager.rs, service_manager.rs, package.rs)**:
  - Implemented ActivityManager with multi-task support, Activity backstack pushing and popping, transition state machines (onCreate -> onStart -> onResume / onPause -> onStop -> onDestroy), and hardware back key dispatch (handle_back_pressed).
  - Implemented ServiceManager managing background/foreground Android Services (ServiceRecord), startService lifecycle (onCreate, onStartCommand), stop_service (onDestroy), and foreground notification promotion (start_foreground).
  - Implemented package.rs supporting .obb expansion file parsing (ObbFile) and XAPK bundle container assembly (XapkBundle) handling split APKs (ABI/density splits).
  - Integrated ctivity_manager and service_manager into AndroidRuntime (launcher.rs), and added unit tests.
- 2026-11-08: **OpenGL ES & EGL Context Translation Layer (gl.rs)**:
  - Implemented OpenGlEsEngine, EglContext, and EglWindowSurface managing EGL context state machines and window surfaces.
  - Implemented core GLES state management (GlesState): viewport configuration, clear color handling, buffer clear commands, and draw call tracking (gl_draw_arrays).
  - Implemented gl_swap_buffers presenting the 32-bit RGBA backbuffer directly into the Stride presentation Surface.
  - Integrated gl_engine into AndroidRuntime (launcher.rs), exported C FFI bindings (stride_runtime_gl_swap_buffers, stride_runtime_gl_viewport, stride_runtime_gl_clear in stride_dex_ffi.h and lib.rs), and added unit tests.
- 2026-11-08: **Bionic Libc Emulation & Native Shared Library Linker (`bionic.rs`)**:
  - Implemented `BionicEnvironment` with core symbol emulation for liblog (`__android_log_print`, `__android_log_write`), libm (`sin`, `cos`, `tan`, `atan2`, `sqrt`, `pow`, `floor`, `ceil`), and libc (`strlen`, `abort`).
  - Built `NativeLinker` connecting `stride_elf_inspector` and `stride_native_loader` to parse ELF shared libraries, resolve dynamic symbols, DT_NEEDED dependencies, and JNI function bindings.
  - Added `load_native_library` in `AndroidRuntime` (`launcher.rs`), exposed `stride_runtime_load_so` in C FFI (`stride_dex_ffi.h`, `lib.rs`), and wired `ExecutionBackend.swift` to extract and link native ARM64 `.so` libraries directly from APK archives.

- 2026-11-08: **Android Canvas 2D Graphics Engine & Orientation Handling (`canvas.rs`, `graphics.rs`)**:
  - Implemented `Canvas`, `Paint`, `PaintStyle`, `RectF`, and `Path` with matrix transformation stack (translate, scale, save, restore) rasterizing 2D primitives directly into the 32-bit RGBA `Surface` framebuffer.
  - Added `ScreenOrientation` configuration change handling on `DisplayMetrics` and `Surface.resize()`. Commit `3e69f51` pushed to `feat/dex-class-method-loader`.

- 2026-11-07: **Managed Heap String Interning, JNI Native Extensions & Runtime Diagnostics**:
  - **Managed Heap (`heap.rs`)**: Added string interning (`intern_string`), string concatenation (`concat_strings`), substring extraction, length calculation, `array_copy` slice transfers, byte array allocation/extraction (`allocate_byte_array`, `get_byte_array`), and identity hash code generation to `ManagedHeap`. Added tests for interning and array copy.
  - **JNI Native Subsystem (`jni.rs`)**: Added local reference frame push/pop (`push_local_frame`, `pop_local_frame`), field access helpers, byte array manipulation, mangled symbol lookup (`Java_pkg_Class_method`), and standard built-ins (`java.lang.Math` min/max/abs/sqrt, `System.arraycopy`, `android.os.SystemClock.uptimeMillis`).
  - **Runtime Diagnostics (`diagnostics.rs`)**: Built `RuntimeDiagnostics` aggregator tracking missing APIs, unlinked classes, unresolved methods, warnings, and fatal errors with severity levels and summary telemetry.
  - **Telemetry & UI Integration**: Connected `missing_apis_count` from `AndroidRuntime` through C FFI (`stride_dex_ffi.h`, `lib.rs`) to `ExecutionBackend.swift` and displayed live missing API counts in the iOS app UI (`ContentView.swift`).

- 2026-11-07: **Expanded Dalvik Opcode Coverage in ArtVm (`interpreter.rs`)**:
  - Implemented 2-addr arithmetic (`0xb0..=0xcf` for int, long, float, double).
  - Implemented 3-register float and double binary arithmetic (`0xa6..=0xaf`).
  - Implemented unary operations (`neg-long`, `not-long`, `neg-float`, `neg-double`) and full conversions (`long-to-int/float/double`, `float-to-int/long/double`, `double-to-int/long/float`).
  - Implemented `filled-new-array` (0x24), `filled-new-array/range` (0x25), and `fill-array-data` (0x26).
  - Added unit tests for 2-addr arithmetic and filled array execution. Commit `45becac` pushed to `feat/dex-class-method-loader`.

- 2026-11-07: **Major ART-Compatible Managed Runtime & Execution Pipeline Integration**:
  - **Blocker Resolution**: Fixed E0502 Rust borrow-checker conflict in `interpreter.rs` by extracting method metadata upfront, decoupling `self.methods` queries from the recursive execution frame and `JniEnv` creation.
  - **Exception Unwinding & Stack Traces**: Connected recursive method calls and `athrow` with try/catch handler search and frame recording in `RuntimeStackTrace`.
  - **Multi-DEX Class Loader & Linking (`loader.rs`)**: Implemented `AppClassLoader` capable of loading multiple DEX files, indexing class definitions, mapping code items to executable `MethodCode`, linking class hierarchies, and executing static class initializers (`<clinit>`).
  - **Comprehensive Android Framework Subsystem (`framework.rs`)**: Implemented type-safe `Bundle`, `Intent` component resolution, `PackageManager` (`PackageInfo`, `ActivityInfo`, `ServiceInfo`), `AssetManager`, `Resources`, atomic `SharedPreferences`, and complete `ActivityLifecycleState` state machines.
  - **Graphics, Input & Audio Subsystem (`graphics.rs`)**: Added `DisplayMetrics`, 32-bit RGBA `Surface` framebuffer with pixel manipulation, `MotionEvent` (multi-touch with coordinates and pressure), `KeyEvent` (hardware back/menu/volume), thread-safe `InputQueue`, and `AudioTrack` buffer management.
  - **End-to-End APK Execution Coordinator (`launcher.rs`)**: Created `AndroidRuntime` which parses `AndroidManifest.xml` via `stride-axml`, loads multi-DEX files, resolves main launcher activities, runs `<clinit>`, allocates the Activity on the managed heap, executes `onCreate(Bundle)` and `onResume()`, and returns detailed `LaunchReport` diagnostics.
  - **C FFI Boundary (`dex-ffi`)**: Added `stride_runtime_create`, `stride_runtime_load_dex`, `stride_runtime_launch_apk`, `stride_runtime_dispatch_touch`, and `stride_runtime_destroy` in `stride_dex_ffi.h` and `lib.rs`.
  - **Swift ART Execution Backend (`ExecutionBackend.swift` & `ContentView.swift`)**: Replaced `UnavailableExecutionBackend` with `StrideArtExecutionBackend` which unzips APKs, extracts multi-DEX binaries and `AndroidManifest.xml`, invokes `stride_runtime_launch_apk`, and displays live execution metrics and lifecycle state directly in the iOS app UI.

- 2026-11-07: Added packed-switch/sparse-switch payload decoding, check-cast/instance-of type verification, athrow exception unwinding through try/catch blocks, 64-bit wide long arithmetic (0x9b..=0xa5), floating point comparisons, invoke-super and invoke-interface dispatch to ArtVm in `interpreter.rs`, and re-exported all core ART runtime subsystems from `dex-runtime`'s root library.

- 2026-11-07: Built and verified the core ART-compatible managed runtime subsystems in `runtime-rust/crates/dex-runtime`. Implemented Dalvik 32-bit register file and 64-bit wide value pairs (`types.rs`), bounded managed heap with instance/array/string allocation and root-tracing mark-sweep garbage collection (`heap.rs`), class inheritance hierarchies, virtual method overriding, super dispatch, and interface resolution (`hierarchy.rs`), thread synchronization and monitor enter/exit locking (`sync.rs`), try/catch protected blocks and stack trace unwinding (`exceptions.rs`), JNI native method registry and JniEnv boundary (`jni.rs`), Android Activity lifecycle, Context, and SharedPreferences (`framework.rs`), and the ArtVm opcode execution engine (`interpreter.rs`). **PR workflow run #136 (run ID 42386497978, commit 0ec243d) passed all 81 Rust unit tests** (4 AXML, 2 FFI, 26 DEX inspector, 43 DEX runtime, 1 ELF inspector, 5 native loader). Run: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42386497978.

- 2026-11-07: Added a bounded object identity/runtime-class store and `new-instance` allocation to `dex-runtime`, plus `invoke-virtual` and `invoke-virtual/range` dispatch by exact runtime class and method-reference index. Added explicit dispatch registration, null/missing-dispatch/unknown-type errors, and tests for allocation, exact-class implementation selection, and failure cases. **PR workflow run #131 passed all 65 Rust unit tests** (4 AXML, 2 FFI, 26 DEX inspector, 27 DEX runtime, 1 ELF inspector, 5 native loader); unsigned iOS IPA job skipped. Run: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42373868648. This is not a managed heap: no GC, constructors, inheritance/interface lookup, invoke-super, or full Android class loading.

- 2026-11-07: Added bounded `invoke-direct` and `invoke-direct/range` support to the integer DEX interpreter. The receiver is passed as the first incoming register word, null receivers are rejected, caller-supplied method bodies execute in isolated frames, and the existing shared integer field store is available to the callee. Added tests for direct-call instance field reads, range-form receiver/argument passing, and null receiver rejection. Run #124 exposed two incorrectly encoded test fixtures; those encodings were corrected in commit b2458fb. **Run #125 passed all 61 Rust unit tests**, including all 23 DEX runtime tests; unsigned iOS IPA job skipped. Run: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42373460547. No virtual dispatch is claimed or implemented.


- 2026-11-07: Added a bounded static-call execution path to dex-runtime. The interpreter now supports invoke-static (35c) and invoke-static/range (3rc) against caller-supplied in-DEX method bodies, isolated recursive call frames, DEX incoming-argument placement at the high end of callee registers, integer move-result, shared field storage across frames, aggregate instruction budgets, missing-method/argument-shape errors, and a 64-frame recursion limit. Added tests for both invoke formats, argument passing/return values, missing bodies, result misuse, and recursion limits. PR workflow run #122 passed all 58 Rust unit tests (4 AXML, 2 FFI, 26 DEX inspector, 20 DEX runtime, 1 ELF inspector, 5 native loader); unsigned iOS IPA job was skipped. Run: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42373078738. This is still a deliberately limited integer interpreter: no invoke-virtual/direct dispatch, object return values, wide arguments/results, exception propagation, Android framework, JNI, or native execution.


- 2026-11-07: Implemented integer-only DEX field execution in `dex-runtime`: `iget`, `iput`, `sget`, and `sput` now use isolated instance/static stores; null instance references are rejected; typed linking gates storage to `I/Z/B/C/S`. Corrected the DEX register validator's field instruction formats (22c instance vs 21c static). Fixed the DEX metadata imports and corrected earlier fixture expectations/encodings. **PR workflow run #120 passed all 53 Rust unit tests** (4 AXML, 2 FFI, 26 DEX inspector, 15 DEX runtime, 1 ELF inspector, 5 native loader); iOS/IPA job skipped as intended. Run: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42372536386. This remains a limited integer interpreter primitive, not ART or an Android app runtime.

- 2026-11-07: Added a bounded integer field-storage layer to the small DEX interpreter. It now executes the `iget`, `iput`, `sget`, and `sput` integer opcode forms against separate static storage and per-object instance storage; null instance receivers fail explicitly. Added tests for instance/static round-trips, object isolation, and null receivers. The DEX register validator was corrected to treat instance field opcodes as 22c (two nibble registers) and static field opcodes as 21c (one byte register), rather than incorrectly treating all field opcodes as 23x. Added a typed resolver that limits this storage path to `I/Z/B/C/S`; wide, reference, inherited, volatile, and framework field semantics remain unsupported. This is a controlled interpreter primitive, not general Android execution. New code and tests have not yet been compiled or run in CI.

- 2026-11-07: Added bounded structural instruction-stream decoding for each non-empty method and checked incoming register layout against method prototypes. The check accounts for instance receivers and wide (`long`/`double`) incoming parameters. The partial operand-bounds pass covers 12x, 11x/11n, 22x, 32x, selected 21x, 22t/22c/22s, 23x, and 22b formats. Follow-ups add explicit register-list validation for 35c/45cc and contiguous register-window validation for 3rc/4rcc, plus two-register bounds checks for selected wide moves, constants, array/field operations, conversions, and long/double arithmetic. Try/catch parsing validates ordered non-overlapping try ranges, handler-list structure, encoded handler offsets, catch target addresses, and instruction-boundary alignment for protected-range starts/ends and catch targets. Class-data parsing now rejects direct/virtual method category mismatches and duplicate method definitions within a class. These are still partial structural checks: instruction-boundary checks for try/catch addresses, invoke argument-width checks against resolved prototypes, and other verifier rules remain pending. This is not full Dalvik verification, class loading, or execution. The invoke, wide-pair, and try/catch changes are committed but have not yet been tested in CI.

- 2026-11-07: Expanded DEX class-data parsing to retain each declared field's field_id, name, descriptor, access flags, and static/instance classification. Parsing now rejects field definitions whose `ACC_STATIC` flag disagrees with the encoded static/instance list and rejects duplicate field indices across both lists. The class registry now resolves declared fields and DEX `field_id` references by exact owner/name/type; tests cover static and instance field lookups, invalid field indices, and exact-type mismatch. This remains metadata linking only: no field storage, inherited-field lookup, `iget/iput/sget/sput` execution, or Android boot class path. These changes have not been compiled or run in CI.

- 2026-11-07: Added the first metadata-backed `ClassRegistry` in `runtime-rust/crates/dex-runtime`. It atomically registers class definitions from DEX inventories, rejects duplicate descriptors, and resolves declared methods by exact owner/name/prototype or by a DEX `method_id` index. Tests cover successful app-method lookup, exact-prototype mismatch, invalid indices, duplicate registration, and expected unresolved Android framework references when no boot class path is registered. This is not bytecode invocation, inheritance/virtual dispatch, field linking, or Android framework support. These new Rust tests have not been run in CI.

- 2026-11-07: Added resolved DEX `method_id` and `field_id` inventories to `ClassInventory`. Each reference now exposes its declaring class descriptor, member name, and prototype/type, including references to framework/library classes not defined in the current DEX. Added a method-reference fixture assertion. This is identifier resolution groundwork for class loading and invoke/field linking; it does not execute methods, resolve runtime classes, or prove HCR compatibility. The DEX identifier reference changes are included in the passing Rust workflow run #120.

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
- Recent code step: APK import invokes the Rust DEX inventory FFI with bounded extraction; Rust workspace tests pass in run #120, but Swift/Xcode integration still needs its own device-build checkpoint.
- Latest implementation: class inventory decodes method instruction streams, validates incoming register-word layout, checks selected register formats including invoke/range lists/windows and selected wide pairs, structurally validates try/catch ranges/handlers and instruction-boundary alignment, checks direct/virtual method classification plus duplicate definitions, and now resolves method_id/field_id entries to declaring-class descriptors, names, prototypes, and field types. This is still static metadata; next build the minimum runtime class/method resolution path against a controlled fixture, then use the exact pinned HCR bundle to determine which framework/native dependencies are actually needed. The invoke/wide/try-catch batch and `ClassRegistry` tests passed in Rust-only CI run #120; never enable `build_ios` for routine Rust checks. The registry resolves exact declared methods and fields and deliberately leaves Android framework/inherited references unresolved. Next implement a controlled static-method invocation/call-frame fixture; keep the current field interpreter limited to integer-like primitive fields. The field parser/registry and integer field-execution bulk passed Rust CI in run #120 (53 unit tests). Static invoke/call-frame support passed run #122 (58 total Rust unit tests). Bounded invoke-direct passed run #125 (61 total). Object allocation plus exact-runtime-class invoke-virtual dispatch passed run #131 (65 total). No managed heap/GC, constructors, superclass/interface dispatch, invoke-super, object-valued returns, wide arguments/results, or exception propagation is implemented. The CI fixes included module-qualified imports, a test return-register correction, field reference count correction, and code-item error propagation/fixture fixes. Review the new Swift tests at a later iOS integration checkpoint. Obtain/pin the exact official APK before writing HCR-specific app-execution code.


## Latest CI failure diagnosis (2026-11-07)

Workflow run #86 (`42368614689`) failed in the Rust tests: 18 passed and 2 failed. Both failures came from an incorrectly encoded `const/4` test fixture, not from the register validator: DEX format 11n stores register A in bits 8–11, while the original fixture `0x1012` put `1` in the literal nibble (bits 12–15), leaving the register as `v0`. Corrected both fixtures to `0x0112` in `lib.rs` and `classes.rs`. This correction has not yet been rerun in CI; do not mark tests green until a new run passes.


---

## Phase 2 implementation handoff (2026-11-09)

### Branch and workflow rules
- Active development branch: `feat/dex-class-method-loader`.
- Stable branch: `main`. It has not been changed or merged as part of this batch.
- Keep all follow-up implementation and fixes on the development branch. Merge only after the relevant checks pass and the user has validated the IPA on the iPhone.
- Do not cancel an Actions run merely because its macOS IPA job is queued. Keep the active run enabled and let GitHub's hosted macOS runner pick it up.

### Phase 2 code changes now on the development branch
Latest branch HEAD at this handoff update: `7abf5e92ceaba89962d26d76737508240697d30c`.

- DEX class loading and hierarchy:
  - Corrected superclass resolution to interpret `superclass_idx` as a DEX `type_idx`, resolving the descriptor from the DEX type table rather than treating it as a class-definition index.
  - Parsed and retained each class's interface descriptors from `interfaces_off`, with bounds/alignment/count/type-index validation.
  - Carried interface descriptors through the loader into runtime class metadata.
  - Virtual dispatch excludes private/direct and static methods from override candidates.
- Guest touch input:
  - Registered `View.setOnTouchListener` and stored the listener on the managed View.
  - Added touch-listener callback dispatch for down/move/up/cancel, including a managed MotionEvent with action and coordinates.
  - Touch listeners can consume the event and prevent the click callback from also firing.
  - Hit testing recognises Views with either click or touch listeners.
- Multi-DEX metadata:
  - Added a method-ID-to-owning-DEX index in the app class loader.
  - Attached all DEX inventories to ArtVm and switch the active string/type/method/field reference tables according to the currently executing method's owning DEX.
  - Added regression coverage proving methods read their own DEX string tables.
  - Follow-up audit should still examine nested invocation/class-initializer context and any remaining first-DEX-only resolver paths before treating multi-DEX as fully complete.
- Native keyboard bridge for EditText:
  - Added EditText focus hit-testing and focused guest text state.
  - Added C FFI `stride_runtime_set_focused_text`, header declaration, and Swift execution-backend wrapper.
  - Added an iOS keyboard/TextField overlay in the execution UI, with focus and dismissal handling.
  - Native keyboard text is currently copied into the guest EditText's managed `mText` field and triggers redraw. This is an initial bridge, not full Android InputConnection/IME, selection/composition, key-event, or input-filter emulation.
- TextWatcher:
  - Registered `addTextChangedListener`, `removeTextChangedListener`, and `getText` bridges.
  - Added initial callback dispatch for `beforeTextChanged` and `onTextChanged`.
  - **Known incomplete behavior:** `afterTextChanged` is not yet dispatched, and callback ordering/old-new text semantics require correction. Do not claim full TextWatcher compatibility until fixed and covered by regression tests.
- Regression tests:
  - Added a multi-DEX string-table ownership test.
  - Added a focused EditText keyboard-update test.
  - The first test attempt failed because the EditText test fixture had no enabled-state field set. Commit `7abf5e9` initializes `mEnabled = 1` in that fixture; this was a test-fixture correction, not evidence that a real APK's input path is fully working.

### CI / IPA status at handoff
- Earlier workflow run `42618368837` failed in Rust runtime tests: 100 passed, 1 failed. Failure: `focused_edit_text_receives_native_keyboard_updates` expected the fixture to hit-test as an enabled EditText but got `None`.
- Corrective commit: `7abf5e92ceaba89962d26d76737508240697d30c` — initializes the fixture's `mEnabled` field.
- Latest workflow run: `42618604737` (run #417), commit `7abf5e92ceaba89962d26d76737508240697d30c`.
  - Rust runtime test job: **SUCCESS**.
  - iOS unsigned IPA job: **QUEUED** at last check; it has not completed, so no IPA artifact is confirmed yet.
  - Keep this run active; do not cancel or dispatch a duplicate IPA workflow while this one is queued.
  - Workflow URL: https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/42618604737
- Previous workflow `42618009233` was cancelled during the intermediate implementation batch; it is historical and is not the active build.
- Draft PR #1 remains open and unmerged. Do not merge to `main`.

### Remaining work / next session
1. Leave workflow `42618604737` running until the IPA job starts and completes. Then verify the artifact exists and record its exact download URL/expiry.
2. Audit multi-DEX metadata ownership through nested method calls and class initialization; ensure method resolver and every type/field/string reference uses the correct DEX inventory.
3. Correct TextWatcher callback semantics: call `beforeTextChanged` before mutation, update managed text, call `onTextChanged`, then `afterTextChanged`; preserve correct old/new text and lengths. Add focused regression tests.
4. Verify keyboard FFI status handling and ensure no repeated log spam when no guest EditText is focused. Do not describe the current overlay as a complete Android IME.
5. Add touch-listener dispatch regression tests, including consumed versus unconsumed touch and click suppression.
6. After the remaining implementation/audit batch, run one consolidated final test checkpoint and one IPA workflow. Avoid repeated test runs during small edits, per the user's requested workflow.
7. Physical iPhone test is still required: import a real calculator APK and confirm displayed text is legible, the input field is initially correct, typed values update the guest app, button taps execute guest callbacks, and no built-in fake calculator/demo screen is shown. CI passing alone does not establish real APK compatibility.

### Accuracy boundary
The latest green Rust workflow confirms the current Rust test suite passes on the tested commit. It does **not** prove that the keyboard bridge, TextWatcher, touch dispatch, multi-DEX behavior, or a real calculator APK works end-to-end on an iPhone. The IPA build is still queued, and device verification remains pending.


---

# Full development handoff — all repository updates from 2026-11-08 through 2026-11-09

This section is the consolidated day-to-day record, not just the Phase 2 subset. It was compiled from the development-branch commit history for Nov 8 and Nov 9. The commit history contains many small implementation, regression-fixture, compiler-fix, and integration commits; related commits are grouped below so the next session can resume without overlooking work outside Phase 2.

## Branch, scope, and current revision
- Repository: https://github.com/strideproject77-lgtm/Stride-Official
- Active branch: `feat/dex-class-method-loader`.
- Stable branch: `main`, still unchanged by this work. The observed main HEAD remains `6aa5594c2719b19a33565a3278ce2937ad1e15c3`.
- Development HEAD when this handoff was written: `0b29780bfb55474e111907ab5eede9f723b6e29d`.
- Keep all new implementation and bug fixes on the development branch. Do not merge PR #1 until checks pass and the resulting IPA has also been physically checked on the iPhone.
- User's preferred verification cadence: batch the remaining implementation, then do one consolidated final test/IPA checkpoint. Do not cancel an already queued IPA workflow; do not launch duplicate builds just to try to clear the queue.

## Nov 8 — runtime framework and app-execution groundwork

### Runtime and Android framework
- Added a managed Android View/Activity content bridge, including stateful View properties and Activity content APIs.
- Added Activity lifecycle superclass bridges and execution of `onStart`; retained the AndroidRuntime handle after launch so later event/input calls can continue against the same runtime.
- Persisted app static fields across lifecycle interpreter calls.
- Connected the Activity `onCreate` framework super call and resolved DEX invoke references before ART dispatch.
- Added a real Pocket Ridge DEX loader regression fixture and a real Pocket Ridge Android binary-manifest (AXML) regression fixture. These are controlled loader/parser fixtures, not a claim that Pocket Ridge runs on-device.
- Improved native-library loading behavior to continue best-effort APK launch after a native-library load failure, while reporting that native loading failed or was only planned/inspected rather than implying the library executed.
- Added Android Paint framework bridge and corrected the default Paint color fixture.
- Earlier Phase 3 groundwork in this period also includes ActivityManager task/backstack lifecycle handling, ServiceManager start/stop/foreground promotion, and XAPK/split-APK/OBB package container handling.

### DEX, AXML, and runtime diagnostics
- Fixed DEX `goto/10t` branch-offset decoding.
- Improved AXML string-pool parsing, UTF-16 length handling, header offsets, node bounds, and parser regression fixtures.
- Added classified runtime faults and actionable launch diagnostics, covering archive/native failures, DEX loading, interpreter faults, unresolved methods, and lifecycle failures.
- Added persistent, batched per-session diagnostics with event identifiers, archive rotation, detailed runtime checkpoints, and export of the complete ART/runtime report after launch.
- Exposed diagnostics export through C FFI and the Swift session-log UI; corrected C header declarations, header guards, Swift interpolation, Codable event identifiers, and log line breaks.
- Added per-APK crash reports and categorized runtime diagnostics.
- Added a real-manifest/DEX regression path and more precise loader failure messages. Diagnostic output should distinguish static inspection, metadata loading, lifecycle readiness, and active guest execution.

### Graphics, native and compatibility subsystems
- Added Android Canvas 2D rasterization with Paint, RectF, Path, matrix transforms, and Surface output; added Android orientation configuration handling and Surface resizing.
- Added Bionic libc/libm symbol emulation for the implemented common symbols, plus a native shared-library linker that inspects ELF shared libraries, DT_NEEDED dependencies, dynamic symbols and JNI bindings; connected APK ARM64 `.so` extraction/loading through the Swift execution backend.
- Important boundary: this is a bounded initial symbol/runtime layer, not full Bionic, complete ELF relocation support, or proof that arbitrary native libraries execute correctly.
- Added EGL/OpenGL ES initial state/context/window-surface handling, viewport/clear/draw tracking and buffer presentation into Stride's RGBA Surface.
- Added ActivityManager backstack and ServiceManager integration, XAPK split assembly and OBB parsing.
- Added end-to-end compatibility integration coverage that exercises the current orchestration for selected Bionic symbols, SharedPreferences, Canvas rasterization, EGL/GLES clear/swap, MotionEvent/InputQueue, ServiceManager lifecycle, FPS telemetry and ANR watchdog. This verifies those test paths, not arbitrary APK compatibility.
- Added performance telemetry: rolling FPS estimates, 1% low frame-time/FPS calculation, heap allocation reporting, frame recording FFI, and a 5-second ANR watchdog; connected available metrics to the Swift execution UI.

### Diagnostics and test/CI corrections
- Fixed Swift interpolation in multi-DEX gap and runtime log diagnostics.
- Corrected diagnostic export assertions and stable DEX failure wording.
- Corrected a Rust `goto/10t` decoder bug and AXML fixture/offset issues.
- Fixed compatibility-test APIs/borrows and ensured ActivityManager/ServiceManager use the VM-owned heap.
- Fixed the OpenGL color-buffer FFI flag and SharedPreferences `commit()`/`apply()` API bridges.
- Restored manual-only IPA workflow behavior after temporary validation-trigger commits; the workflow policy at the time required explicitly enabling the iOS build input.

## Nov 8 — user-facing iOS app, importing, execution UI and design
- Reworked APK import to copy document-picker files safely into the app sandbox, removed duplicate import controls, improved selection reliability, added import progress/completion feedback, and moved archive inspection off the main thread with concurrency-safe result handling and cleanup for incomplete imports.
- Improved launcher/library layout and responsive game-library home; added switchable grid/list layouts, import/delete actions, safer removal of imported APKs, and removed low-value technical APK details from launcher cards.
- Added persistent force-execution session history and TXT log export UI, plus a dedicated black APK execution session screen and hideable live status bar.
- Added structured launch event logs, touch-event tracing without MOVE-event spam, native-library error reporting, and explicit lifecycle-ready status wording.
- Added selectable themes and palettes, including clean white, warm dark bronze, exact Indigo Navy, and purple hero variants; updated the selected/default theme during the day's iterations.
- Added theme customization/accent settings and made the redesigned settings/header shortcut interactive.
- Generated and selected Stride app icon variants, added the iOS AppIcon asset catalog and metadata, and added workflow-side icon generation/verification.
- Corrected adaptive Android launcher icon extraction to composite foreground and background rather than stripping the original icon background; expanded launcher image discovery across resource variants.
- Corrected document picker/package detail sheet presenter conflicts and made the package detail sheet follow the selected theme.
- Fixed iOS 15 SwiftUI compatibility issues (kerning/letter spacing/button modifiers) and Swift compilation issues in the log-export sheet.
- Hardened unsigned IPA workflow configuration, ARM64 executable architecture verification, and removed duplicate Rust test execution from the IPA job.

## Nov 9 — execution screen interaction and device behavior
- Added the guest Activity touch-event pump, exposed it through C FFI, connected it to the iOS display-link scheduler, and wired touch input plus frame/event pumping into the execution screen.
- Added hit-testing and guest View click-listener dispatch, including tests for hit-testing and background rendering.
- Added common Android Button/EditText constructors, LinearLayout/ViewGroup constructors, TextView constructors and TextView text/style getters.
- Added MotionEvent JNI accessors needed by touch-driven APKs, removed duplicate registrations, and corrected `getActionMasked()` action masking.
- Added ViewGroup hierarchy/recursive Activity lookup, child mutation methods, bounded Android binary XML layout-tree parsing, and a bounded layout inflater model.
- Added ViewGroup layout and Surface background rendering, then connected the resumed Activity View tree to the Surface renderer. The guest framebuffer is bridged into SwiftUI.
- Added weighted Android LinearLayout layout and per-edge margins/layout params, TextView/ImageView state/resource hooks, and Activity Surface rerendering after each guest event pump.
- Added Android string/color resource lookup from `resources.arsc`, APK drawable decoding into guest image resources, and a runtime image-resource bridge; removed the temporary image placeholder.
- Added calculator-oriented Java String and Double bridges, including both Java String substring overloads, safer return values/borrows, numeric/string JNI bridge coverage, and DEX floating-point comparison opcode dispatch fixes.
- Added calculator display rendering regression coverage, calculator glyph pixel-decoding coverage, and corrected guest touch coordinate mapping. Native iOS system fonts were added for managed text rendering.
- Added Activity constructor execution before `onCreate`, DEX type/field index regression coverage, preservation of DEX identifier table indices, and persistent static field/DEX metadata propagation through lifecycle callbacks.
- Added inherited Activity Window framework method resolution and framework bridge resolution when linked ancestor classes are absent.
- Fixed duplicate MotionEvent JNI registration, test result extraction, fixture syntax, and canvas rendering tests.
- Added three-finger execution chrome toggle and moved its gesture into the SwiftUI gesture arena; corrected triple-tap/toggle behavior in earlier iterations, and improved execution chrome placement below the portrait notch/safe area.
- Kept iOS auto-rotation, presented execution full-screen, and corrected orientation restoration for no-sensor/locked policies and repeated close cleanup. APK manifest screen orientation is applied to the iOS execution scene; explicit Info.plist orientation support was added.
- Redesigned the Stride app icon in additional iterations and polished the library.
- Added native keyboard focus and text input bridge for guest EditText (details and known limitations are in the Phase 2 section above).

## Nov 9 — DEX runtime correctness, class loading and input follow-ups
- Fixed floating-point DEX comparison dispatch and several JNI String/Double bridge issues.
- Added Activity constructor dispatch before lifecycle callbacks and corrected class/type/field reference table handling.
- Added DEX-local string/type/method/field reference preservation and resolution, including field operands through `field_ids`, persistent static fields and DEX metadata passed to Activity callbacks and class initializers.
- Added runtime virtual/interface dispatch and corrected virtual-dispatch regression fixtures.
- Corrected superclass descriptor resolution using DEX `type_idx`, parsed and linked interface declarations, resolved inherited interface default methods, and excluded private/direct/static methods from virtual override candidates.
- Added View.OnTouchListener registration and callback dispatch for down/move/up/cancel, including managed MotionEvent values and event-consumption/click-suppression behavior.
- Added per-method owning-DEX metadata for multi-DEX execution and switched active DEX reference tables while executing methods.
- Added native iOS keyboard overlay/focus behavior, focused guest EditText tracking, FFI text updates, and initial TextWatcher registration/callback support.
- Added multi-DEX and focused EditText regression tests. One test initially failed because its fixture omitted the enabled-state field; commit `7abf5e9` corrected the fixture by setting `mEnabled = 1`. The latest Rust test workflow subsequently passed.
- Exact commit history for this date is available on the branch; the entries above consolidate the implementation themes rather than reproducing every tiny commit title.

## Workflow and verification status at this handoff
- Latest relevant Rust verification: workflow run `42618604737`, commit `7abf5e92ceaba89962d26d76737508240697d30c`; Rust runtime test job succeeded (101 runtime tests reported in the run summary).
- The unsigned iOS IPA job in run `42618604737` was still **QUEUED** at last check. No IPA artifact from that run is confirmed until its macOS job completes and the artifact list is checked.
- The handoff documentation update commit `0b29780bfb55474e111907ab5eede9f723b6e29d` triggered a separate push workflow `42619217671`, which was **PENDING** at last check. This is a docs-only push and must not be confused with the IPA job from run `42618604737`.
- Workflow `42618009233` was an earlier cancelled run; do not treat it as the active IPA build.
- Do not cancel run `42618604737` to clear the queue and do not start duplicate IPA runs while it is queued.
- `main` remains at `6aa5594c2719b19a33565a3278ce2937ad1e15c3`; no merge has been performed.

## Remaining work / explicit limitations
1. Let the active IPA workflow finish; then inspect the macOS job and verify whether an unsigned IPA artifact was actually uploaded. Record artifact name, size, and expiry only from the completed run.
2. Continue the multi-DEX audit: nested invocation context, class initialization, method resolver ownership, and any remaining first-DEX-only paths.
3. Complete Android TextWatcher semantics and regression tests: `beforeTextChanged` must run before mutation, then update the text, run `onTextChanged`, then `afterTextChanged`; validate old/new text and lengths.
4. Complete guest EditText behavior beyond direct string replacement: selection/cursor, composing text, InputConnection/IME, key events, filters and error/status handling. Current keyboard bridge is only initial text synchronization.
5. Add explicit View.OnTouchListener tests for consumed/unconsumed events and click suppression.
6. Continue framework coverage only against observed APK dependencies; do not infer support from constructors or static UI rendering alone.
7. Test the real calculator APK on the iPhone: verify readable text, correct initial display, keyboard entry updates the guest app, calculator buttons invoke guest callbacks, and no built-in demo/fake calculator is displayed.
8. Test a clean minimal Java APK and then the chosen real target APK. Record exact APK version/hash, ABI, API level, device/iOS version and observed result.
9. Keep all fixes on `feat/dex-class-method-loader`; merge only verified passing changes to `main`.

## Truthful compatibility boundary
The commits from Nov 8–9 add substantial UI rendering, Android framework, event/input, DEX loading, JNI, graphics, native-linker, diagnostics, and telemetry groundwork. Passing unit/integration tests validate only the exercised paths. They do not establish general Android compatibility, arbitrary native library execution, complete ART behavior, or a successful real APK launch on the iPhone. Do not report a compatibility percentage based only on code volume or commit count.

## External APK scan artifacts

- 2026-11-09: Added a static package scan for the user-provided **2048 v1.7.0 APKPure XAPK**. Full manifest, split inventory, DEX/dependency inventory, assets, required Android APIs, implementation priorities, and real-device acceptance checklist:
  - [2048 v1.7.0 XAPK compatibility scan](https://github.com/strideproject77-lgtm/Hill-Climb/blob/hill-climb-game/docs/test-apks/2048-1.7.0-apk-analysis.md)
  - Package: `com.tpcstld.twozerogame`, versionCode 1013; minSdk 23, targetSdk 37.
  - XAPK SHA-256: `e3a9c160ea34bfa916e0022dcacb3da07732042c1f2912f52b7bbdc4485ec3a7`.
  - Key runtime profile: one DEX 035 file (6,502 class definitions; 51,183 method IDs), 24 app-owned classes, AppCompatActivity + custom View/Canvas 2D renderer, touch/swipe input, Kotlin/coroutines and AndroidX/Google Play Services dependencies, 17 language splits plus required xxhdpi density split.
  - No native `.so` libraries were found in this XAPK; prioritize DEX class resolution, XAPK/resource assembly, Activity/AppCompat startup, Canvas 2D, touch, lifecycle, and persistence before native ELF work for this target.
  - **Static scan only**: this is not evidence that Stride has launched or supports the APK. Real-device gameplay and save/restore tests remain required.



## Nov 12 — HCR ARM64 native execution foundation (implementation-only; not tested)
- Added a bounded AArch64 guest interpreter with address generation, integer arithmetic/logical operations, branches, literal/unsigned loads and stores, and integer register-pair stack operations.
- Added strict guest memory page permissions, execute-only instruction fetch checks, bounded guest stack allocation, and a hard per-call instruction budget.
- Added ELF-backed guest image materialization with PT_LOAD mapping, supported relocation application, page permission mapping, exported-symbol indexing, and an adapter for already-relocated NativeLinker modules.
- Added AAPCS64 x0-x7 integer/pointer argument setup, JNI short/long symbol name resolution, and guest invocation entry points for exported functions, JNI_OnLoad and statically-mangled JNI methods.
- Added regression tests and updated runtime/HCR documentation. These tests were **not run**; no IPA workflow was started and no IPA was built.
- Important boundary: this is a limited interpreter, not full ARM64/ART/Android. SIMD/FP, JavaVM/JNIEnv semantics, RegisterNatives, JNI reference conversion/callbacks, native constructors, guest threads/TLS/syscalls, complete Bionic, Cocos2d-x, EGL/GLES2 rendering and the HCR frame loop remain incomplete. HCR has not yet reached its original loading screen.
- Implementation commits in this batch: `b9875d5`, `151c5ca`, `51d2238`, `44d0e84`, `28da0aa`, `7d7440f`, `2f6635f`, `24d0e8d`, `6bd242f`, `8a2f966`, `e03feea`, `a91f01d`.

- Follow-up in the same batch: added bounded `JNINativeMethod` array parsing and staged dynamic native registration lookup. It is not yet connected to the actual `JNIEnv->RegisterNatives` callback; cross-module pointers and JNI reference conversion remain incomplete. No tests/workflows/IPA were run.


### Follow-up implementation batch — ARM64 coverage and ELF constructors

- Extended the bounded interpreter with logical-immediate operations, conditional-select instructions, narrow byte/halfword memory operations, and unscaled LDUR/STUR forms. Added a regression test for logical-immediate mask decoding.
- Preserved DT_* dynamic metadata in loaded native modules; guest-image creation now discovers and validates DT_INIT and DT_INIT_ARRAY entries against mapped executable pages.
- Added an explicit constructor invocation method. The higher-level loader must call it only after successful dependency relocation and in dependency-first order; this method does not itself load missing dependencies or implement a thread scheduler.
- This is still a limited integer AArch64 interpreter. SIMD/FP, atomics, TLS, syscalls, real JavaVM/JNIEnv callbacks, cross-module guest pointer space, complete Bionic, NativeActivity and EGL/GLES rendering remain blockers.
- Tests were added but not run. No workflow or IPA build was started.

- Added SBFM/BFM/UBFM bitfield instructions and sign-extending LDRSB/LDRSH/LDRSW unsigned-offset forms.

- Added `NativeLinker::dependency_initialization_order` to expose dependency-first ordering for already-loaded native modules. Missing DT_NEEDED entries remain explicit and must be checked before constructor execution; cross-module guest memory and call bridging are still incomplete.


### JNI validation and registration follow-up

- Added strict JNI method descriptor parsing for primitive, object and array parameter/return types. Malformed descriptors and void parameters are rejected.
- Added checked JNI_OnLoad version negotiation for JNI 1.1, 1.2 and 1.4; unsupported version returns are rejected rather than accepted as success.
- Added class-scoped UnregisterNatives state handling and duplicate-entry rejection inside one RegisterNatives table. Registration remains staged atomically.
- These are validation/state-management foundations, not a real guest JNIEnv/JavaVM function table. Guest-callable JNI trampolines, callback dispatch, managed-object/reference conversion and native-to-managed callbacks remain unimplemented.
- Regression tests were added but not run. No workflow or IPA build was started.


## Latest JNI local-reference lifecycle batch
- Added checked local-frame allocation, `NewLocalRef`-style reference retention, `DeleteLocalRef`-style removal, local GC-root snapshots, and result-preserving frame pop.
- Added tests for reference deletion/root behavior, frame result preservation, and invalid state/handles. Tests are deliberately deferred to the single final test pass.
- Scope is deliberately honest: this is Rust-side JNI groundwork, not a guest `JNIEnv*` function table, global/weak reference implementation, or automatic native-to-GC root integration.

### JNI failure-safety refinement


### Opaque guest JNI reference handles
- Added an exported `JniReferenceTable` that maps opaque 64-bit guest tokens to managed objects without exposing internal heap IDs as `jobject` values.
- Supports local/global/weak-global lifetime classes, local frames, result-preserving frame pop, clone/delete operations, identity checks, live-handle resolution, strong-root snapshots and weak-reference pruning.
- Cleared weak references resolve as NULL but retain their token until `DeleteWeakGlobalRef`, matching JNI weak-handle lifetime more closely.
- This does not yet implement guest-callable `JNIEnv*`/`JavaVM*` function tables, executable trampolines, native-to-managed callback dispatch or automatic GC root handoff. Tests are added but deliberately unrun until the final single test pass.

- Added strong global-reference create/delete and local+global strong-root snapshots; weak globals and automatic GC integration remain outstanding.
- `PopLocalFrame(result)` validates and reserves before changing frame/reference stacks, preventing partial state changes on failure.
- String/int-array/byte-array creation reserves local-reference capacity before heap allocation; null local-reference operations follow JNI behavior.
- Added/updated regression coverage only; tests remain deferred to the one final pass.



---

## ChatGPT Mobile Roadmap — Common Compatibility Engine

**Purpose:** Build reusable Android runtime capabilities that improve compatibility across many apps. Each numbered phase is one small implementation batch that can realistically be completed, reviewed, and reported in a single assistant work session. Do not use broad subsystem-sized phases.

### Branch and execution rules
- All coding and bug fixes happen only on `feat/dex-class-method-loader`; `main` remains stable.
- Keep each phase to one narrowly scoped behavior change (normally 1–3 closely related tasks) with explicit acceptance criteria and regression coverage.
- Complete the phase's full defined scope before starting the next phase in the same work session when feasible.
- Avoid repeated test runs after every edit; perform one consolidated Rust workflow validation after the implementation batch.
- Do not build, sign, export, or publish an IPA without explicit user permission.
- Do not claim compatibility from imports, static analysis, stubs, or lifecycle-only execution.
- After each meaningful batch, update this handoff with code changes, tests, remaining limitations, and extra improvements.
- Mark a phase **Completed** only when its entire small scope is implemented and reviewed. Workflow validation is tracked separately.
- Do not merge into `main` until the consolidated validation passes and the diff is reviewed.

### Manageable phase tracker

| Phase | Single-batch scope | Status |
|---|---|---|
| 1 | Class-loader safety: sticky `<clinit>` failure state and atomic DEX registration | Completed — implementation reviewed; previous Rust workspace workflow passed |
| 2 | Managed-heap safety: reject oversized arrays before backing allocation; validate zero-length array-copy arguments | Completed — implementation and regressions added; Rust workspace workflow passed |
| 3 | JNI-aware managed-heap collection: include legacy and opaque JNI strong refs; clear weak refs after sweep | Completed — implementation reviewed and regressions added; consolidated workflow pending |
| 4 | DEX virtual dispatch: choose the maximally-specific interface declaration and reject ambiguous defaults | Completed — implementation reviewed and regressions added; consolidated workflow pending |
| 5 | DEX hierarchy linking: preserve original method access flags so abstract/private/static declarations remain distinguishable | Implemented — focused regression added; consolidated workflow pending |
| 6 | DEX invoke-kind validation: reject static/instance and direct/virtual/interface mismatches against linked access flags | Implemented — focused regressions added; consolidated workflow pending |
| 7 | Reject dangling managed-object references in fields and reference arrays | Completed — regression coverage and consolidated Rust workflow passed |
| 8 | Repair stale object edges that bypass checked setters during GC; preserve valid cyclic reachability | Completed — regression added; consolidated Rust workflow #672 passed |
| 45 | Exact HashMap no-argument constructor compatibility for AndroidX lifecycle startup | Completed — consolidated CI validated; device retest pending |
| 46 | Managed HashMap get/put/remove/contains/size compatibility subset for observer maps | Completed — consolidated CI validated; device retest pending |
| 47 | AndroidX observer-map compatibility: exact HashMap.putIfAbsent behavior plus truthful guest-render status | Completed — consolidated CI validated; device retest pending |
| 48 | Correct putIfAbsent behavior for null-valued and absent mappings; add selectable app surface themes | Completed — consolidated CI validated; device retest pending |
| 49 | AndroidX LifecycleRegistry ArrayList constructor and indexed state-stack operations; truthful startup-blocked UI report | In Progress — Rust runtime suite passed (227/227 in dex-runtime); authorized consolidated iOS compile + unsigned IPA build triggered; device retest pending |
| 50 | AndroidX lifecycle WeakReference constructor plus inherited Reference.get/clear; surface first unresolved startup API in runtime UI | In Progress — Rust workspace tests and physical iOS compile passed in workflow #914; fresh device retest pending |

**Phase sizing rule:** A phase must be small enough to finish in one implementation reply. If a task needs multiple independent subsystems, split it into later phases rather than expanding the current phase.

### Phase 1 — Class-loader safety
**Acceptance criteria**
1. A failing `<clinit>` transitions to a terminal error state; retry must not report success or rerun partial initialization.
2. DEX class/method/index registration is atomic: a failure must not leave partially committed loader state.
3. Targeted regressions cover both behaviors.

**Implementation log — 2026-11-13 — Completed**
- Implemented sticky class-initializer failure state and contextual error reporting.
- Made DEX registration transactional by staging classes, methods, lookups and DEX-owner indices before committing.
- Added regressions for failed initializer retry and method-ID exhaustion with unchanged loader registries.
- Extra improvements: fixed stale native-loader test fixtures; separated JNI opaque reference handles from 32-bit object IDs; deduplicated local/global GC roots.
- Files: `dex-runtime/src/loader.rs`, `bionic.rs`, `native_execution.rs`, `jni.rs`.
- Previous validation: Rust workspace workflow passed; `stride-dex-runtime` had 164 passing tests. IPA job skipped.
- Phase 1 is complete because its two acceptance criteria and regressions are implemented. The current combined workflow after Phase 2 remains a separate validation gate.

### Phase 2 — Managed-heap allocation safety
**Acceptance criteria**
1. Oversized array requests are rejected against the managed-heap budget before allocating the backing vector.
2. Array-copy validates both array references and positions even when the requested copy length is zero.
3. Regression tests prove rejection does not mutate heap object count/allocated-byte accounting and zero-length copies still validate arguments.

**Implementation log — 2026-11-13 — Completed (workflow pending)**
- Added checked array-size calculation and managed-heap capacity preflight before creating the backing `Vec`; oversized requests return `HeapError::OutOfMemory` without host allocation pressure.
- Kept allocation accounting consistent by recording the preflighted size only after the backing object is created.
- Changed zero-length `array_copy` to validate source/destination handles and bounds before returning.
- Added regression tests for an `i32::MAX` array request against a 64-byte heap and invalid references/out-of-range positions in zero-length copies.
- Extra improvement: array-copy range-end arithmetic now uses checked addition to avoid integer-overflow acceptance.
- File: `runtime-rust/crates/dex-runtime/src/heap.rs`.
- Validation: workflow run #644 (commit `144b92e5ce0ce2f5015922d78961ad4999598797`) passed the complete Rust workspace test suite; `stride-dex-runtime` passed **166 tests, 0 failed**. The unsigned iOS IPA job was **Skipped**. No IPA build/sign/export occurred.

### Phase 3 — JNI-aware managed-heap collection
**Acceptance criteria**
1. A single JNI-owned collection entry point includes caller-supplied VM/register roots plus all live legacy and opaque JNI local/global strong references.
2. Weak JNI references do not keep objects alive and are cleared to NULL after their referents are collected, while their tokens remain deletable.
3. Root snapshots are deterministic and deduplicated; focused regressions cover strong-root survival, weak-root clearing, and duplicate references.

**Implementation log — 2026-11-13 — Completed and validated**
- Added `JniEnv::collect_garbage(additional_roots)` to combine caller roots with legacy local/global roots and opaque guest local/global roots before invoking the managed heap collector.
- After the sweep, the entry point prunes collected weak JNI referents so their still-valid opaque handles resolve as NULL.
- Made the combined strong-root snapshot sorted and deduplicated, preventing duplicate root entries when one object is retained through multiple JNI reference kinds.
- Added regressions proving local, legacy-global, opaque guest-global, and caller-provided roots survive; weak-only objects are reclaimed and weak handles resolve as NULL; duplicate strong handles yield one root.
- Extra improvement: all callers using the new JNI-aware entry point get one consistent GC lifecycle instead of having to manually assemble roots and remember weak-reference pruning.
- Extra GC improvement in this combined batch: `ManagedHeap::collect_garbage` now removes stale interned-string cache entries after sweep, preventing dead string keys/handles from accumulating. Regression verifies reclaimed entries are removed and reinterning produces a fresh live object.
- Files: `runtime-rust/crates/dex-runtime/src/jni.rs`, `runtime-rust/crates/dex-runtime/src/heap.rs`.
- Latest implementation commits: `e578d89bd0c4312d21d1cb6d911732ca7c81656b` and follow-up regression coverage `b9da80154618c7d2e1f9f2a90a03a8236fcd44a9`.
- Validation: complete Rust workspace workflow run #664 (`43461894563`) passed on this branch; Rust runtime tests succeeded. The iOS/IPA job was skipped. No IPA build, signing, or export was run.

### Phase 4 — Most-specific interface default-method dispatch

**Acceptance criteria**
1. Class declarations retain precedence over interface defaults, with static/private methods excluded from virtual dispatch.
2. Among inherited interface declarations, a strict subinterface declaration takes precedence over its parent declaration.
3. A more-specific abstract redeclaration suppresses a parent default; unrelated maximal defaults are not selected based on traversal order.
4. Focused regressions cover child-default precedence, abstract suppression, and unrelated-default ambiguity.

**Implementation log — 2026-11-13 — Completed and validated**
- Reworked `ClassHierarchy::resolve_virtual_method` to collect reachable interface declarations and select only maximally-specific declarations using interface assignability.
- Preserved class-chain precedence and excluded static/private methods. Abstract interface declarations now participate in specificity, so they can correctly suppress inherited defaults.
- Ambiguous unrelated maximal declarations return unresolved instead of silently selecting whichever interface happened to be visited first.
- Added three regressions for subinterface default precedence, abstract redeclaration suppression, and unrelated default ambiguity.
- Extra improvement: interface dispatch is now deterministic and independent of the order in which interface descriptors are traversed.
- Files: `runtime-rust/crates/dex-runtime/src/hierarchy.rs`.
- Implementation commits: `335f61fac0fcdc513947e267ee29ed9b60a7f26e`, `10b92c50f401b081f7a02c7c9400506912f02d7f`.
- Validation: complete Rust workspace workflow run #664 (`43461894563`) passed on this branch; Rust runtime tests succeeded. The iOS/IPA job was skipped. No IPA build, signing, or export was run.

### Phase 5 — Preserve DEX method access flags during hierarchy linking

**Acceptance criteria**
1. Linking loaded DEX classes into `ClassHierarchy` preserves each method's original access flags rather than replacing them with `public`.
2. Abstract interface redeclarations remain abstract after linking and therefore suppress inherited interface defaults.
3. Private and static method flags are preserved for later access/invoke-kind checks.
4. Regression coverage verifies abstract, private, and static flags through the actual loader-to-hierarchy linking path.

**Implementation log — 2026-11-13 — Completed and validated**
- Added `RuntimeClass::register_method_with_flags`; the existing `register_method` remains source-compatible and delegates with its previous public default.
- Updated `AppClassLoader::link_hierarchy` to pass `LoadedMethod.access_flags` for both virtual and direct methods.
- Added a loader-level regression that links an abstract subinterface method plus private and static direct methods, checks their flags, and verifies that the abstract declaration suppresses the parent interface default after linking.
- Extra improvement: downstream access checks and invoke-kind validation now have the real DEX flags available instead of fabricated public flags.
- Files: `runtime-rust/crates/dex-runtime/src/hierarchy.rs`, `runtime-rust/crates/dex-runtime/src/loader.rs`.
- Implementation commits: `8762d631bebe3fcd2a3e56a7a55fafd03020b99a`, `b5fc4b6095373e7ba6bf86a3da11c13be0a7f250`, `fa0002b79f9df93e052fb58c8efaef6ec77ba986`.
- Validation: complete Rust workspace workflow run #664 (`43461894563`) passed on this branch; Rust runtime tests succeeded. The iOS/IPA job was skipped. No IPA build, signing, or export was run.

### Phase 6 — DEX invoke-kind and access-flag compatibility

**Acceptance criteria**
1. For linked app declarations, `invoke-static` must target static methods; instance invoke forms must not target static methods.
2. `invoke-direct` must target a direct private instance method or constructor; virtual/interface/super forms must reject direct or private declarations.
3. `invoke-virtual` and `invoke-interface` must agree with whether the referenced owner is a class or interface.
4. Framework declarations without a materialized runtime class remain eligible for exact-signature JNI/framework bridge resolution.
5. Focused regressions cover static/instance mismatch, private/direct rules, constructor invocation, and class/interface dispatch-kind mismatch.

**Implementation log — 2026-11-13 — Completed and validated**
- Added invoke-kind validation at the DEX interpreter call site using the preserved `RuntimeClass` method access flags.
- Rejects incompatible static, direct, virtual, interface, and super invoke forms before argument decoding or dispatch, with call-site PC and target signature in the error.
- Keeps framework/JNI bridge resolution working when no linked app declaration exists.
- Added focused regressions for static-vs-instance mismatch, private direct calls, constructors, and class/interface opcode compatibility.
- Extra UI improvement: refreshed the Home welcome hero from the old purple/blue treatment to a dark walnut-brown gradient with warm amber highlights, carrying the same palette into the mini app mark and CTA.
- Files: `runtime-rust/crates/dex-runtime/src/interpreter.rs`, `Sources/StrideApp/ContentView.swift`.
- Tests: focused regressions included in successful consolidated Rust workspace workflow #663. Swift UI compile remains unverified because the IPA job was skipped.
- IPA: not built, signed, or exported.

### Phase 7 — Reject dangling managed-object references

**Acceptance criteria**
1. Instance fields cannot store non-null object handles that are not currently live in the managed heap.
2. Array elements cannot store non-null dangling object handles; bounds/type validation remains intact.
3. Failed reference writes are atomic and leave the prior field/element value unchanged.
4. Null object references remain valid, and valid object edges remain discoverable by mark-sweep GC.
5. Focused regressions cover both instance-field and reference-array paths, including reachability after collection.

**Implementation log — 2026-11-13 — Completed and validated**
- ManagedHeap.set_instance_field now rejects non-null Value::Object handles that do not resolve to a live heap object.
- ManagedHeap.set_array_element validates array type and bounds, then validates object handles before mutating the element.
- Failed reference writes return HeapError::InvalidHandle and preserve the previous value.
- Added regressions proving invalid field/array references are rejected and valid child objects remain reachable when the owner/array is a GC root.
- Extra UI/UX improvement: Home and Library no-results states now offer a one-tap “Clear search” action; the Home search field has an explicit accessibility label and hint.
- Files: runtime-rust/crates/dex-runtime/src/heap.rs, Sources/StrideApp/ContentView.swift.
- Validation: consolidated Rust workspace workflow #667 (run 43464328643) passed; IPA job was skipped. Swift UI compile remains unverified; no IPA build, signing, or export was run.

### Phase 8 — Repair stale managed-object edges during collection

**Acceptance criteria**
1. Before mark traversal, collection repairs non-null object handles in live instance fields and arrays when the referenced handle is no longer present in the managed heap.
2. Valid object edges and null references remain unchanged.
3. Root-reachable cyclic graphs and their valid descendants survive collection, while dangling edges are cleared to `Value::Null`.
4. A focused regression exercises the public mutable-object access path that can bypass checked field/array setters.

**Implementation log — 2026-11-13 — Completed and validated**
- Added a GC pre-trace sanitation pass over instance fields and array elements. Any non-null `Value::Object` pointing to a missing heap handle is replaced with `Value::Null` before reachability traversal.
- This closes an escape-hatch gap: checked setters already reject dangling references, but runtime subsystems using the public mutable-object accessor could otherwise inject stale edges that remained observable from a live owner.
- Added a regression that deliberately inserts stale handles through mutable access, then verifies GC clears them while preserving an owner → array → child graph with a back-edge cycle.
- Extra UI/UX improvement: the Library search field now has a one-tap clear control while a query is present, plus explicit accessibility label and hint.
- Files: `runtime-rust/crates/dex-runtime/src/heap.rs`, `Sources/StrideApp/ContentView.swift`.
- Implementation commits: `9288ba03fc66b8fb159b7fda73af316dd5f3eb6a`, `7bada546071596ab264de81730fb9a6746d51918`, `2fedc9b79729755f65b18792449cd3cd26ae4496`.
- Tests: consolidated Rust workspace workflow #672 (run ID `43466701440`) passed, including the new regression. The iOS/IPA job was skipped. Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 9 — JNI weak-reference promotion semantics

**Status: implementation complete and Rust-validated.**

- Fixed resolve_guest_ref so a weak global reference whose referent has already been reclaimed resolves as NULL, even if collection was invoked through the lower-level heap API before the JNI post-GC pruning pass.
- NewLocalRef-style promotion from that cleared weak reference now creates the null handle (0) instead of returning a stale-object error.
- Strong references that point to reclaimed objects still report an explicit invariant error; unknown/deleted guest tokens remain errors.
- Added a focused regression covering stale weak promotion, weak-vs-null identity, and stale strong-reference rejection.
- Extra Library UI improvement: when search is active, show a live filtered-results count (matches out of total) with an accessibility announcement and numeric transition.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Validation: consolidated Rust runtime workflow #679 (run ID 43470693805) passed, including this JNI regression. The iOS/IPA job was skipped; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 10 — Keep handled exceptions out of later stack traces

**Status: Completed (Rust workspace validation passed).**

- Fixed nested DEX call exception unwinding: the caller records the stack-trace depth before entering a child method.
- If the child exception is caught by the caller, only frames appended by that handled child unwind are removed. Existing outer frames remain intact.
- If the exception is not caught, the caller frame is appended as before, preserving the uncaught trace from throw site outward.
- Added a regression: one child exception is caught, then a second exception escapes; the second trace must contain only its own caller frame and correct DEX code-unit offset.
- Extra Diagnostics UI improvement: Unified Diagnostics now shows live fatal and warning counts alongside retained-event/error totals, making exception/crash triage quicker.
- Files: runtime-rust/crates/dex-runtime/src/interpreter.rs, Sources/StrideApp/ContentView.swift.
- Validation: consolidated Rust runtime workflow #679 (run ID 43470693805) passed, including the new exception-trace regression. The iOS/IPA job was skipped; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 11 — Monotonic Android SystemClock uptime

**Status: Completed (Rust workspace validation passed).**

- Replaced the Android SystemClock.uptimeMillis bridge's wall-clock epoch source with a process-relative monotonic Instant clock.
- Wall-clock timestamps remain available through java.lang.System.currentTimeMillis; adjusting the device clock can no longer make uptimeMillis jump backwards or forwards.
- Added a regression that checks non-negative monotonic results and confirms uptime remains distinct from epoch wall-clock time.
- Extra Diagnostics UI improvement: added a one-tap SUMMARY copy action containing total events, fatal/error/warning counts, and the 12 most recent diagnostic entries. Existing full-copy and export actions remain available.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Validation: consolidated Rust workspace workflow #701 (run ID `43481239721`) passed. The macOS unsigned IPA/UI compile job is tracked separately in the same run; final result pending.
- IPA: not built, signed, or exported.

### Phase 12 — Android Context package identity

**Status: Completed (Rust workspace validation passed).**

- Added per-JNI-environment Android package identity configuration through set_package_name, with basic Java package-segment validation.
- Implemented Context.getPackageName() and Context.getOpPackageName() bridges. Both return guest-managed Java String objects from the configured package identity.
- Calls made before package bootstrap now fail explicitly instead of returning a fake/placeholder package name.
- Added regression coverage for configured identity, both Context methods, missing bootstrap identity, and malformed package names.
- Extra Settings UI improvement: Runtime Status now lists the Context package identity bridge as a distinct capability rather than hiding it inside the broad Android framework status.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Integration note: the APK/application bootstrap must call JniEnv::set_package_name with the parsed manifest package before app Context code is executed. This batch adds the bridge/configuration API; it does not claim the full app bootstrap wiring is already connected.
- Validation: consolidated Rust workspace workflow pending; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 13 — Manifest-backed Context bootstrap

**Status: Completed (Rust workspace validation passed).**

- Added JniEnv::from_manifest(heap, metadata), directly consuming stride-axml ManifestMetadata and configuring the Context package identity from the parsed AndroidManifest.xml package attribute.
- Bootstrap fails closed when the manifest package attribute is missing or invalid; it does not invent a fallback package.
- Added an end-to-end JNI regression using ManifestMetadata, verifying Context.getPackageName returns the package parsed/configured from manifest metadata and that missing package identity is rejected.
- Extra Settings UI improvement: Runtime Status now names the actual Manifest → Context bootstrap path instead of listing only the lower-level Context methods.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Integration boundary: APK startup code that owns parsed ManifestMetadata must construct its JNI environment through JniEnv::from_manifest; this batch exposes and tests that direct handoff API but does not claim every APK launch path already calls it.
- Validation: consolidated Rust workspace workflow pending; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 14 — Manifest-backed PackageInfo mapping

**Status: Completed (Rust workspace validation passed).**

- Added PackageInfo::from_manifest(&ManifestMetadata) to build the framework PackageInfo record directly from parsed AndroidManifest.xml metadata.
- Maps package identity, version code/name, min/target SDK, permissions, application class, launcher Activity, Activities and Services.
- Activity records preserve per-component orientation and fall back to manifest-level orientation; service records preserve exported state. Components outside the current PackageInfo model are intentionally not misrepresented as activities or services.
- Missing package identity is rejected. Missing optional version/SDK values use explicit conservative defaults; missing exported values default to false in the current bool-based model.
- Added regression tests for full metadata mapping, component filtering/defaults and missing package identity.
- Extra Settings UI improvement: Runtime Status now exposes PackageInfo metadata mapping as a separate capability.
- Files: runtime-rust/crates/dex-runtime/src/framework.rs, Sources/StrideApp/ContentView.swift.
- Integration boundary: this batch creates a reliable PackageInfo record from manifest metadata; it does not yet implement guest Java PackageManager object construction or all PackageManager query methods.
- Validation: consolidated Rust workspace workflow pending; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 15 — Bounded PackageManager package-info lookup

**Status: Completed (Rust workspace validation passed).**

- Added AndroidEnvironment::load_package_info_from_manifest to attach PackageInfo derived from parsed ManifestMetadata only when its package identity exactly matches the active app environment.
- Added AndroidEnvironment::get_package_info as a bounded current-app PackageManager.getPackageInfo-style query.
- Unknown package names return PackageNotFound; absent/invalid manifest metadata returns MetadataUnavailable. Mismatched manifest identity is rejected before mutating environment state.
- Added regressions for successful version metadata lookup, unknown package rejection, missing metadata and mismatch atomicity.
- Extra Settings UI improvement: Runtime Status now shows a separate PackageManager lookup capability card.
- Files: runtime-rust/crates/dex-runtime/src/framework.rs, runtime-rust/crates/dex-runtime/src/lib.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: this is a typed Rust framework query API, not yet a guest Java PackageManager object/JNI method bridge; third-party package enumeration and flags overloads remain unsupported.
- Validation: consolidated Rust workspace workflow pending; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 16 — Guest Java PackageManager.getPackageInfo bridge

**Status: Completed (Rust workspace validation passed).**

- Connected manifest-backed metadata to JniEnv::from_manifest. The JNI environment now carries typed PackageInfo only when its package identity matches the configured app identity.
- Added Context.getPackageManager() returning a guest-managed PackageManager object.
- Added the classic PackageManager.getPackageInfo(String, int) handler.
- Materializes guest-managed PackageInfo, ApplicationInfo, ActivityInfo and ServiceInfo objects, plus requestedPermissions String[] and activity/service arrays. Object references are stored in the managed heap graph for GC tracing.
- Rejects null package names, missing metadata and unknown packages explicitly; no placeholder package metadata or third-party package enumeration is fabricated.
- Added regression coverage for manifest mapping into guest objects, nested arrays/fields, version values, unknown package rejection and missing metadata.
- Extra UI/diagnostics improvements: Runtime Status now exposes the guest Java PackageManager capability; PackageInfo/PackageManager/ApplicationInfo log entries are categorized under a dedicated PackageManager diagnostics stage.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: only the active app's manifest-backed metadata is supported. Package visibility, package enumeration, permission-flag filtering, signatures, install/update times and the complete Android PackageInfo field surface remain unsupported.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported by this implementation batch.

### Phase 17 — Guest Java PackageManager.getApplicationInfo bridge

**Status: Completed (Rust workspace validation passed).**

- Added the classic PackageManager.getApplicationInfo(String, int) handler.
- Reuses the same validated current-app package identity and manifest-backed metadata path as getPackageInfo.
- Returns the ApplicationInfo guest object populated with packageName, className, minSdkVersion and targetSdkVersion.
- Unknown packages, null package names and unavailable manifest metadata remain explicit errors.
- Added regression coverage for ApplicationInfo object type, SDK levels and application class mapping.
- Extra UI improvement: the Runtime Status capability card names both guest getPackageInfo and getApplicationInfo JNI methods.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: this phase does not yet implement ApplicationInfo resource paths, flags, nativeLibraryDir, sourceDir, enabled state or label/resource resolution.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 18 — Context app-private directory paths

**Status: Completed (Rust workspace validation passed).**

- Added Context.getFilesDir() and Context.getCacheDir() guest JNI bridges.
- Each method returns a guest-managed java.io.File object carrying the app-scoped Android-style path derived from the configured manifest package identity.
- Added File.getPath(), getAbsolutePath() and toString() accessors for these runtime-created File objects.
- Calls before package bootstrap fail explicitly; no fake path is returned.
- Added regression coverage for both directories, path accessors and missing package identity.
- Extra UI improvement: Runtime Status now lists app-private files/cache path support and clearly states that disk persistence is not implemented yet.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: these are guest path objects only. File creation, directory existence, read/write streams, mkdirs, deletion and durable sandbox-backed storage remain unsupported.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 19 — Extended guest app-private File path behavior

**Status: Completed (Rust workspace validation passed).**

- Added Context.getNoBackupFilesDir() and Context.getCodeCacheDir() using package-scoped Android-style paths.
- Extended runtime-created guest java.io.File objects with getName(), getParent(), isAbsolute() and getAbsoluteFile(), alongside the Phase 18 path accessors.
- Added regressions for no-backup/code-cache paths, path names, parent paths and absolute-path behavior.
- Extra UI refinement: Runtime Status now lists all four app-private directory categories while explicitly retaining the no-disk-persistence limitation.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: paths remain guest metadata only. Directory creation, file I/O, canonical path resolution, permissions and durable storage are not implemented.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 20 — PackageManager launcher Intent resolution

**Status: Completed (Rust workspace validation passed).**

- Added guest PackageManager.getLaunchIntentForPackage(String) using only the active app's parsed manifest launcher activity.
- Builds a guest-managed Intent with ACTION_MAIN, CATEGORY_LAUNCHER, package identity and ComponentName; all object references remain connected through the managed heap for GC tracing.
- Added Intent.getAction(), getPackage(), getComponent(), getFlags() and hasCategory(), plus ComponentName.getPackageName() / getClassName().
- Returns Java null when the current package has no declared launcher activity; rejects null package names, unknown packages and incompatible receiver types explicitly.
- Added regression coverage for launcher intent fields, category lookup, component identity, no-launcher behavior and unknown-package rejection.
- Extra UI improvement: Runtime Status now surfaces launcher Intent resolution as a separate capability.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: only the current app's manifest-declared launcher is resolved. Cross-package intent resolution, intent filters beyond the parsed launcher, chooser/system activities and actual host-side activity dispatch remain unsupported.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 21 — Guest Intent construction and mutation

**Status: Completed (Rust workspace validation passed).**

- Added guest Intent constructors for empty and action-based intents.
- Added setAction(), setPackage(), setClassName(String, String), addCategory(), setFlags() and addFlags().
- Category insertion is idempotent; repeated addCategory() calls do not duplicate entries.
- Preserves guest object identity for fluent Intent APIs and stores ComponentName/category references in the managed heap graph.
- Added regression coverage for constructor state, package/class component mapping, category de-duplication and bitwise flag accumulation.
- Extra UI improvement: Runtime Status now separates Intent creation/mutation from manifest launcher resolution.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: this phase does not dispatch activities, resolve external packages, persist intents across process boundaries, or implement Parcelable/Bundle extras.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 22 — Guest Intent extras and Bundle reads

**Status: Completed (Rust workspace validation passed).**

- Added Intent.putExtra() overloads for String, int, long and boolean values.
- Added getStringExtra(), getIntExtra(), getLongExtra(), getBooleanExtra(), hasExtra() and getExtras().
- Added guest Bundle getString(), getInt(), getLong() and getBoolean() read paths over the same managed extras object.
- Missing keys return Android-style null/default values; typed getters do not reinterpret mismatched primitive types.
- Added regression coverage for typed round trips, Bundle access, key presence and default values.
- Extra UI improvement: Runtime Status now exposes typed Intent extras as a separate capability.
- Extra runtime bug fix: added ManagedHeap.has_instance_field() so missing fields are distinguishable from explicitly stored zero/null values; Intent getters now preserve caller defaults and hasExtra() checks true field presence.
- UI polish: redesigned all Runtime Status rows with clearer spacing, readable multi-line descriptions, a consistent READY / IN PROGRESS status pill, and combined VoiceOver labels.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: Parcelable/custom objects, nested Bundles, arrays, URI/ClipData and cross-process serialization remain unsupported.
- Validation: consolidated Rust workspace tests passed in workflow #43484355383 on commit `5198d1c89f32761f8f65d844849be62aedf18798`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 23 — Guest Uri parsing and Intent data/MIME bridge

**Status: Completed (consolidated Rust workspace validation passed).**

- Added guest-managed android.net.Uri StringUri objects and Uri.parse(String).
- Added toString(), getScheme(), getAuthority(), getHost(), getPath(), getQuery(), getFragment(), isAbsolute() and isHierarchical().
- Added Intent.setData(), getData(), getDataString(), getScheme(), setType(), getType() and setDataAndType().
- Enforced Android's data/type mutual-clearing rules for setData() and setType(); setDataAndType() preserves both.
- URI parsing rejects NUL/control characters, supports opaque URIs and authority/host/path/query/fragment extraction.
- Added regression tests for content/web/opaque URI parsing, Intent data round-trip, MIME type clearing and malformed control input.
- Extra UI: Runtime Status now lists URI parsing and Intent data/MIME support separately.
- Files: runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: no URI permission grants, percent-decoding/encoding helpers, Parcelable serialization, ContentResolver or provider dispatch.
- Validation: consolidated Rust workspace tests passed in workflow #43486129688 on commit `839ebf34f9a0d024d46a56d8b3ef066ffb9f6027`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 24 — Bundle key lifecycle and typed defaults

**Status: Completed (consolidated Rust workspace validation passed).**

- Added ManagedHeap.remove_instance_field() and deterministic instance_field_names() support.
- Added Bundle.containsKey(), remove(), clear(), size(), isEmpty(), and getString/getInt overloads with caller-supplied defaults.
- Added Intent.removeExtra() and replaceExtras(Bundle).
- Bundle operations are restricted to Stride's namespaced Intent-extra fields so runtime-owned guest fields are not erased accidentally.
- Added regression tests for key presence, removal, size/empty transitions, and typed default behavior.
- Extra bug fix: field enumeration is deterministic and missing keys are distinguished from explicit zero/null values.
- Extra UI: Runtime Status now surfaces Bundle lifecycle management as a separate capability.
- Files: runtime-rust/crates/dex-runtime/src/heap.rs, runtime-rust/crates/dex-runtime/src/jni.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: primitive/object arrays, nested Bundles, Parcelable values and Android Parcel wire format remain unsupported.
- Validation: consolidated Rust workspace tests passed in workflow #43486129688 on commit `839ebf34f9a0d024d46a56d8b3ef066ffb9f6027`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Phase 25 — Framework Intent data/type and filter semantics

**Status: Completed (consolidated Rust workspace validation passed).**

- Extended the typed framework Intent model with MIME type metadata.
- Added set_data(), set_type(), set_data_and_type() and get_scheme() with Android-compatible data/type clearing behavior.
- Added filter_equals(), which compares action, data URI, MIME type, component and categories while intentionally ignoring extras and flags.
- Added remove_category() for category lifecycle management.
- Added regression coverage for data/type transitions, URI scheme extraction, category removal and filter comparison behavior.
- Extra bug fix: filter matching no longer treats extras or launch flags as identity-defining filter fields.
- Extra UI: Runtime Status now distinguishes framework Intent filter behavior from guest JNI Intent mutation.
- Files: runtime-rust/crates/dex-runtime/src/framework.rs, Sources/StrideApp/ContentView.swift.
- Scope boundary: no full Android resolver, MIME database, URI grant matching, IntentSender, chooser, cross-package dispatch or host activity navigation.
- Validation: consolidated Rust workspace tests passed in workflow #43486129688 on commit `839ebf34f9a0d024d46a56d8b3ef066ffb9f6027`; Swift UI compile remains unverified.
- IPA: not built, signed, or exported.

### Latest validation checkpoint — 2026-11-13

- Final consolidated workflow #43486129688 on commit `839ebf34f9a0d024d46a56d8b3ef066ffb9f6027`: complete Rust workspace test suite **passed** (265 tests across the workspace; zero failures).
- Phases 23–25 are included in that validated source revision; Phases 16–25 are recorded as Completed. Phase 26 is now in progress; its current implementation has not yet been validated.
- The workflow's unsigned iOS IPA job was **skipped**. No IPA was built, signed or exported.
- Swift UI compilation remains unverified because no Xcode/IPA build was run.
- All implementation changes remain on `feat/dex-class-method-loader`; `main` is untouched. The branch has substantial divergence from `main`, so no merge was attempted without a separate reconciliation and review.
- Do not build, sign or export an IPA without explicit user authorization.

### Next small implementation batches
These are candidates, not completed work:
- Activity navigation: connect guest startActivity() to the existing in-process activity dispatcher, with explicit unsupported results for external/system targets.
- Graphics/input: one bounded Canvas, MotionEvent multi-pointer, or surface-lifecycle behavior plus regression.
- Native compatibility: one concrete ELF relocation, Bionic symbol, or JNI native-call behavior plus regression.
- Dedicated UI/UX batch: improve APK library cards, import progress, launch-state feedback and diagnostics presentation with matching tests where practical.
- Compatibility harness: one exact APK scenario and evidence record at a time.




**Implementation update — 2026-11-14 (runtime connection batch):**
- Added `AndroidRuntime::dispatch_activity_intent()`: explicit package/class validation, dot-name to DEX descriptor normalization, class initialization, metadata-aware ArtVm construction, ActivityManager dispatch, and synchronization of `active_activity_obj` / `active_activity_class`.
- Added `AndroidRuntime::handle_activity_back_pressed()`: dispatches Back through ActivityManager, restores the runtime's active Activity pointers, and clears stale input/render hit-test state when the stack empties.
- Added explicit external/system package rejection with structured diagnostics rather than silently routing outside the imported APK.
- Lifecycle reliability fix: Activity constructor, onCreate, onStart/onResume, and finish/back callbacks now return interpreter errors, uncaught guest exceptions, and instruction-budget failures instead of silently reporting success. ActivityManager's Back APIs now return `Result<bool, String>`.
- Extra bug fixes: Back now clears stale pressed/focused view references when the task becomes empty; dispatch logs a stable record ID and target component.
- UI status now distinguishes runtime-owned dispatch/Back wiring from the still-pending guest JNI `Context.startActivity()` entry point.
- Remaining: implement guest JNI `Context.startActivity()` routing into this API; add end-to-end DEX/JNI tests; finish UI back-button/input event hookup; run one consolidated validation and then trigger unsigned IPA build.
- Tests: New routing/Back regression cases added; single consolidated workflow validation pending.
- IPA: User authorized an unsigned build after implementation and consolidated validation; no signed/exported IPA is permitted.



**Implementation update — final validation batch (2026-11-14):**
- ActivityManager now executes the Activity constructor before lifecycle creation and propagates interpreter errors, uncaught guest exceptions, and lifecycle instruction-budget exhaustion for constructor/onCreate/onStart/onResume and finish/back callbacks.
- Back APIs now return explicit `Result<bool, String>` outcomes; AndroidRuntime consumes the result and keeps its active Activity pointers synchronized.
- Added regression coverage for rejecting external/system package targets without mutating the task and clearing stale Activity/input state when Back is pressed on an empty task.
- Remaining known limitation: guest DEX calls to `Context.startActivity(Intent)` are not yet routed through a pending-JNI-event queue into `dispatch_activity_intent()`. This must remain clearly reported as pending; do not describe all Android in-app navigation as connected.
- Validation: first consolidated run #727 found four integration compile errors (Result propagation and metadata borrow lifetime); those exact sites are fixed by commits 9307515 and 1406414. A single consolidated rerun will compile/test the full Rust workspace and, if tests pass, build the unsigned iOS IPA.



**Build failure follow-up — 2026-11-14:**
- Workflow #729: Rust runtime tests passed; iOS compile stopped at a Swift generic type mismatch in bounded resource-image scanning (`min(Int, UInt32)`).
- Fixed by making `maxResourceFilesToScan` an `Int`; no scan limit or runtime behavior changed.
- Next: rerun consolidated Rust + unsigned IPA workflow. No signing, no release export, no main-branch merge.

### Phase 26 — In-process Activity dispatch foundation

**Status: Partial · runtime-owned dispatch and Back synchronization implemented; guest JNI entry point and end-to-end validation remain.**

- Consolidated workflow #724 exposed two Rust type errors: the new `dispatch_method` receiver parameter is `Option<u32>`, but the `onStart` and `onResume` call sites passed bare `u32` values. Both call sites now pass `Some(obj_ref)`; rerun the consolidated workflow before treating Phase 26 as validated. The iOS IPA job was skipped because Rust compilation failed.

- Hardened ActivityManager.start_activity() to require an explicit non-empty component class instead of silently routing implicit intents to a fabricated UnknownActivity.
- Activity allocation now happens before the current top Activity is paused/stopped. Allocation failure returns a diagnostic error and leaves the existing backstack/lifecycle unchanged.
- Activity record IDs now use checked increment so integer overflow cannot silently wrap and collide with earlier records.
- onCreate(Bundle) now receives the Activity receiver plus an explicit null saved-state Bundle on first creation, matching the Android method descriptor.
- Lifecycle callback lookup now walks the loaded class hierarchy so inherited Activity callbacks can resolve.
- Extra bug fixes: missing component and heap allocation failures are surfaced as errors rather than being converted into apparently successful launches; dispatch now avoids mutating the current task before allocation succeeds.
- Extra UI: Runtime Status now lists in-process Activity backstack support separately and clearly labels the guest Context.startActivity bridge as pending.
- Files: runtime-rust/crates/dex-runtime/src/activity_manager.rs, Sources/StrideApp/ContentView.swift, STRIDE_HANDOFF.md.
- Remaining: connect guest Context.startActivity() to the runtime-owned ActivityManager/ArtVm dispatch path; resolve and reject external/system package targets explicitly; propagate guest lifecycle exceptions and synchronize the dispatched top Activity with AndroidRuntime.active_activity_obj/active_activity_class. Add end-to-end JNI dispatch tests.
- Tests: Not run (consolidated validation pending).
- IPA: Not built, signed or exported.

### Named compatibility target list

**These are targets for investigation and eventual real-device testing, not current support claims.** The percentages below are provisional planning estimates only; they must be recalculated from the exact APK version, ABI, Android API requirements, runtime diagnostics and physical-device results. Do not describe them as measured compatibility.

#### Games

| Game | Current planning estimate | Long-term target |
|---|---:|---:|
| 2048 | 40–45% | 90–95% |
| Flappy Bird | 30–35% | 90–95% |
| Subway Surfers | 5–8% | 70–85% |
| Hill Climb Racing | 17–18% | 80–90% |
| Dr. Driving | 8–12% | 70–85% |
| Temple Run 2 | 5–10% | 65–80% |
| Candy Crush Saga | 15–20% | 80–90% |
| Hill Climb Racing 2 | 5–10% | 70–85% |
| Geometry Dash | 10–15% | 75–90% |
| My Talking Tom | 10–15% | 75–85% |
| Ludo King | 15–20% | 80–90% |
| 8 Ball Pool | 5–10% | 65–80% |
| Traffic Rider | 5–10% | 65–80% |
| Extreme Car Driving Simulator | 3–8% | 55–75% |
| Roblox | 1–5% | 40–60% |
| Minecraft | 1–5% | 45–65% |
| Free Fire | 1–5% | 40–60% |
| PUBG Mobile | 1–5% | 35–55% |
| Clash of Clans | 5–10% | 65–80% |
| Brawl Stars | 1–5% | 45–65% |

#### Applications

| App | Current planning estimate | Long-term target |
|---|---:|---:|
| Calculator | 35–45% | 95% |
| Simple Notes | 25–35% | 90–95% |
| Google Keep | 10–15% | 75–85% |
| VLC | 5–10% | 70–85% |
| MX Player | 5–10% | 70–85% |
| Spotify | 5–10% | 65–80% |
| WhatsApp | 5–10% | 60–75% |
| Telegram | 5–10% | 65–80% |
| Instagram | 1–5% | 50–65% |
| Facebook | 1–5% | 50–65% |
| Snapchat | 1–5% | 40–60% |
| TikTok | 1–5% | 45–60% |
| Discord | 5–10% | 60–75% |
| Google Chrome | 5–10% | 65–80% |
| Files by Google | 15–20% | 80–90% |
| Google Translate | 5–10% | 65–80% |
| Microsoft Word | 5–10% | 65–80% |
| Canva | 1–5% | 50–65% |
| CapCut | 1–5% | 40–60% |
| YouTube | 5–10% | 60–75% |

**Compatibility measurement rules**
- A percentage is not a promise that an app will launch. It is a provisional estimate of requirement coverage until supported by test evidence.
- Track each exact APK version/build, package name, ABI, min/target SDK, split set, iOS device/version, and test result.
- Separate import success, static inspection, class loading, Activity lifecycle, first rendered frame, interactive session, and sustained stability.
- Mark an app as **Verified** only after it launches and performs its declared test scenario on a real supported iPhone. Record crashes and missing APIs.
- Revisit the target estimates after the compatibility harness produces evidence. Some apps may remain unsupported because of DRM, Play Services, proprietary APIs, device-specific GPU features, anti-cheat, or unsupported native CPU behavior.

### Progress update format

Append a dated entry under this roadmap after every meaningful implementation batch:

`YYYY-MM-DD — Phase N: [phase title] — Partial / Completed`
- Implemented:
- Files changed:
- Extra improvements:
- Remaining blockers:
- Apps/categories likely helped:
- Tests: Not run / final consolidated test pending.
- IPA: Not built (unless explicitly authorized).

Do not mark a phase complete merely because its files or API names exist. A phase is complete only when its full scope has been implemented and reviewed. The final consolidated validation remains a separate gate before merging any changes into `main`.


**Build failure follow-up — 2026-11-14 (workflow #730):**
- Rust runtime test suite passed (job #132276986512).
- iOS compile failed at `Sources/StrideApp/ContentView.swift:357`: `.contentTransition(.numericText())` requires iOS 16+, while the project deployment target is older.
- Fixed by removing only this optional numeric text transition from the filtered-results count. The live count, filtering, accessibility label, and layout remain unchanged; this avoids raising the deployment target or dropping older-device support.
- Extra UI compatibility fix: kept the Library's filtered-result count and search feedback intact across the supported iOS deployment range.
- Next: run the consolidated Rust + unsigned iOS IPA workflow once against this fix. No signing, no release export, and no merge to main.


---

## Phase 27 — One-Shot Diagnostic Report

**Status: Completed · implementation and consolidated CI validation passed. Real-device report-content review remains a post-build verification item.**

- Goal: make one exported report contain enough session evidence for a first-pass diagnosis, so the user can send one file without being asked for another routine log export.
- Swift report now records the APK SHA-256 using chunked file reads, file size, package format, split count, archive entry count, manifest presence, DEX file/analyzed/skipped counts, class/method inventory, native ABI inventory and resource-table presence.
- Captures Stride app version/build, iPhone model family, iOS version, physical-memory capacity, session duration/status, and returned runtime metrics (classes loaded, instructions, missing APIs, FPS and managed heap bytes).
- Extracts the Android package identity from the captured launch log when it was not passed explicitly; at export time it also queries the retained Rust runtime for a final live diagnostics snapshot, then includes that snapshot, heuristic issue categories, the full raw session log and the structured iOS diagnostic timeline in the same TXT report.
- Expanded heuristic categories for missing Android APIs/classes, DEX/interpreter faults, JNI bridge failures, native dependency failures, graphics/rendering and input/lifecycle errors. Findings remain explicitly heuristic; they are not represented as a proven root cause.
- Rust runtime diagnostic exports now begin with severity totals, missing class/method counts and dropped-event totals before the event timeline; added a regression for the summary counters.
- UI: replaced the ordinary log export action with **Export One-Shot Report (.txt)**; the action is available even if opening the per-session log file failed, and shares the generated report.
- Reliability fix: if the session log file could not be opened, report generation now uses the current in-memory session log instead of accidentally selecting an older report for the same APK.
- Extra improvements: the report fingerprints the exact APK file with SHA-256 so reports from different versions/builds are not accidentally conflated; large APKs are hashed in bounded 1 MiB chunks rather than loaded wholly into memory. Removed a duplicate nested share-sheet presenter so the one-shot report uses the single screen-level share sheet.
- Files: `Sources/StrideApp/Services/StrideDiagnostics.swift`, `Sources/StrideApp/ContentView.swift`, `runtime-rust/crates/dex-runtime/src/diagnostics.rs`.
- Remaining: compile review and the one consolidated Rust + Swift/iOS workflow validation; improve signal/register/native backtrace capture only where the runtime can safely provide it; add structured guest stack frames and per-stage timing as those sources become available.
- Limitation: a single report can provide complete captured evidence, but cannot invent missing OS-level crash data or guarantee a root cause where the runtime never observed the failure. The report explicitly labels heuristic classifications and unavailable fields.
- Tests: added a Rust regression for severity/missing-symbol summary totals; deferred to the single consolidated validation pass; no repeated test runs.
- IPA: not built, signed or exported. `main` remains untouched.


**Phase 27 implementation progress — 2026-11-14:**
- Added a device/environment snapshot to the one-shot report: exact hardware identifier (hw.machine), OS build, display point/pixel dimensions and scale, active/logical CPU counts, thermal state, Low Power Mode, free app-container storage, preferred language and time zone.
- Added SHA-256 and size fingerprints for every supplied split APK, not just the base APK, so split-package reports can be matched to the exact tested package set.
- Added launch-stage evidence coverage for archive/import, manifest, DEX/class loading, ELF/linker, JNI, Activity lifecycle, graphics/framebuffer and input. These are explicitly marked as observed/not captured, not as proof that a stage completed successfully.
- Added a ranked leading diagnostic signal with a suggested next engineering action. It is explicitly heuristic and cannot claim a root cause without corroborating evidence.
- UI extra: report panel explains what the single file contains and the action is labeled Export Complete Diagnostic Report. Renamed output from Crash-Report to Diagnostic-Report so normal/partial sessions are not mislabeled as crashes.
- Files updated: Sources/StrideApp/Services/StrideDiagnostics.swift, Sources/StrideApp/ContentView.swift, STRIDE_HANDOFF.md.
- Remaining: consolidated compile/test review; confirm the report export path in the iOS build; future additions include guest stack frames and per-stage timing, and OS-provided termination diagnostics where safely available.
- Tests: not run by design; one consolidated validation only.
- IPA: not built, signed or exported. main untouched.
- Phase status: Partial — implementation expanded; validation pending.


**Phase 27 reliability follow-up — 2026-11-14:**
- Fixed a session-finalization gap: launch failures/cancellation could leave an active-session marker when the per-session log handle was unavailable or the view disappeared before an execution result existed.
- Session finalization is now idempotent across launch error handling, explicit Close and view dismissal; it closes the session once even when opening its individual log file failed.
- Extra reliability improvement: the next launch no longer treats a failed-to-open log handle as proof that no diagnostic session was active.
- Files: `Sources/StrideApp/Services/StrideDiagnostics.swift`, `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Remaining: one consolidated Rust + iOS compile validation; then review the single-report output from a real device session. Guest stack frames/per-stage timing remain future instrumentation.
- Tests: deferred to one consolidated pass; no intermediate test runs.
- IPA: no build/sign/export in this batch. `main` untouched.
- Phase status: Partial — session lifecycle hardened; validation pending.

**Phase 27 final evidence-completeness fix — 2026-11-14:**
- Fixed a report-content bug found during final review: when the per-session diagnostic file was readable, the report selected it *instead of* the supplied host/session log. That could omit the final live Rust diagnostics snapshot and launch-screen output from an otherwise complete report.
- Report generation now combines the persisted per-session diagnostic stream with the separately supplied host launch log and final runtime snapshot. Findings and stage coverage are computed across both sources, and the report preserves both under an explicit source separator.
- Extra improvement: this closes a blind spot in the exact one-file workflow requested by the user; a successful session-file open can no longer silently suppress the guest runtime snapshot.
- Files: `Sources/StrideApp/Services/StrideDiagnostics.swift`, `STRIDE_HANDOFF.md`.
- Validation: workflow #757 passed the complete Rust runtime test suite, built the iOS arm64 Rust library and XCFramework, generated the Xcode project, compiled Stride for physical iPhones, verified the app bundle, packaged and validated the unsigned IPA, and uploaded the artifact.
- Artifact: `Stride-unsigned-IPA-757` (2,984,175 bytes; expires 2026-11-28). SHA-256 of the artifact archive: `63be9b44d7e24af3086c2bbaac7bbd17f544d77fbf076dc04c79a7f5c459c90d`.
- Remaining post-build verification: inspect a real-device exported TXT report to confirm device-specific values and runtime events are populated; guest stack frames, per-stage timings and OS-provided termination details remain future instrumentation, not blockers for the one-shot captured-evidence scope.
- Tests: consolidated workflow #757 passed. No repeated test pass requested.
- IPA: unsigned build only; no signing or release export. `main` remains untouched.
- Phase status: Completed — implementation, Swift/iOS compilation, Rust validation and unsigned IPA packaging all passed.

**Phase 27 completion — 2026-11-14:**
- Consolidated workflow #757 passed: Rust runtime tests, iOS arm64 Rust build, XCFramework creation, Xcode generation, physical-iPhone Swift compile, app-bundle verification, unsigned IPA package validation and artifact upload.
- Final report fix merges persisted diagnostics with the host launch log and final Rust runtime snapshot, preventing guest evidence from being dropped when the per-session log file exists.
- Phase 27 is Completed for implementation and CI validation. A real-device TXT export review remains a separate post-build verification; the report does not claim to recover OS data or stack frames that were never captured.
- Artifact: `Stride-unsigned-IPA-757`, 2,984,175 bytes, expires 2026-11-28. Archive SHA-256: `63be9b44d7e24af3086c2bbaac7bbd17f544d77fbf076dc04c79a7f5c459c90d`.
- Branch: `feat/dex-class-method-loader`; `main` untouched. No signing or release export performed.


**Unsigned IPA rebuild request — 2026-11-14:**
- User explicitly requested an IPA build. Trigger the existing one-shot CI gate using the exact commit message `Build Calculator validation IPA after tests` so the latest `feat/dex-class-method-loader` source is validated and packaged.
- Build policy: unsigned IPA only; do not sign, export as a release, or merge to `main`.
- Latest known successful unsigned artifact (#757) is older than current branch head; this rebuild is required to include the latest branch changes.


### Phase 28 — Storage & Latest-Session Retention — validation follow-up (2026-11-14)
- Workflow #764 passed both Rust runtime tests and the unsigned iOS build/package job on the feature branch.
- The workflow compiled the Swift app for physical iPhones, verified the app bundle and arm64 executable, packaged the unsigned IPA and uploaded the artifact.
- Remaining device check: restore a previously imported APK/XAPK/APKS/APKM library entry after app relaunch and confirm the zero-byte format marker plus component index restore the expected package type/splits. Existing pre-upgrade folders are cleaned on the next launch.
- Phase 28 remains Partial until package-restoration behavior is verified on-device.

### Phase 29 — Dedicated App Page & Black UI Refresh
**Status: Implementation + CI build validated in workflow #764 · real-device visual review pending.**

- Replaced the app-detail sheet with a dedicated full-screen app page.
- Added package storage-size metadata, including extracted split components, and persistent last-successful-launch timestamp keyed by the canonical package directory.
- Home's Recently Played section now surfaces the latest successfully launched app instead of remaining a static placeholder.
- Added one collapsed Diagnostics disclosure on the app page with captured-event, error and warning totals plus a pointer to the complete one-shot TXT report.
- Removed the theme selector and rotating remote image carousel. The app now uses pure-black surfaces with a warm amber accent and a dedicated full-screen About Stride page.
- Extra UI/UX improvements: last-played is recorded only after the runtime launch call succeeds (not on a failed launch request); full-screen app navigation gives package details and launch controls a consistent dedicated surface; removing the remote carousel also removes network-dependent artwork from About.
- Files: `Sources/StrideApp/ContentView.swift`.
- Remaining: consolidated Swift/iOS + Rust workflow validation; inspect the full-screen app page and About page on a real iPhone; verify size/last-played presentation with plain APK and split packages.
- Historical note: play-history timestamps begin with this build; Stride cannot reconstruct launch dates from before the feature existed.
- Tests: deferred to the single consolidated validation workflow; no intermediate test runs.
- IPA: unsigned build only, already explicitly requested. No signing, release export or main-branch merge.



### Phase 30 — Session Log Copy & Export Reliability
**Status: Implementation committed · consolidated validation pending.**

- Fixed the log export source: the previous Unified Diagnostics Export action pointed directly at the global `stride-diagnostics.log`, which may not contain the active per-session log after the latest-session retention reset.
- Added explicit **COPY LOG** and **EXPORT** actions beside Session Logs so both remain directly accessible without relying on the separate Diagnostics disclosure.
- Copy/export now assemble a single text snapshot from the in-memory session timeline, persisted current-session log fallback, and structured diagnostic entries.
- Export writes a real, non-empty UTF-8 `.txt` file to the app temporary directory, validates that the file exists and has content, then presents that exact file through the iOS share sheet.
- Unified Diagnostics Export now uses the same reliable session snapshot path instead of sharing the potentially empty global log file.
- Extra improvements: export filenames include a sanitized APK name and timestamp; clipboard/export content includes both raw session events and structured diagnostics; accessible labels clarify each action.
- Files: `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Commits: `7e290b1` (restore copy/export actions and create session snapshot); `1012a23` (harden non-empty export-file validation).
- Remaining: review the share-sheet flow and clipboard contents on a real iPhone; verify exporting after a failed launch and after a successful launch; then include this batch in the one consolidated Rust + iOS validation pass.
- Tests: deferred to the single consolidated validation pass; no repeated test runs.
- IPA: not built, signed or exported for this batch. `main` untouched.


### Phase 31 — Native Game Startup: Defer Optional Resource Image Predecode
Status: Implementation committed · consolidated validation and real-device retest pending.

- HCR 1.73.2 real-device session reached archive/split validation, package identity verification, ART runtime creation, asset registration (2,399 assets / 106,702,031 bytes), and resource-table decode (571,000 bytes).
- The user ended the session about five seconds after launch while the backend was still in the eager Android resource-image scan. No DEX load, native-library load, launcher Activity dispatch, JNI initialization or graphics startup event was captured; the report contains no runtime error proving a DEX/JNI failure.
- Optimized Sources/StrideApp/Runtime/ExecutionBackend.swift: native/asset-heavy packages (at least one ARM64 native library and more than 128 bundled assets) now defer optional eager UIKit drawable predecode and proceed directly to DEX initialization. Managed-only packages retain the existing bounded drawable predecode path.
- Added an explicit session-log event with the reason and native-library/asset counts when this predecode is deferred, so the next one-shot report identifies the transition instead of appearing stalled at resource scanning.
- Extra improvement: avoids duplicate startup work for native game engines whose primary assets are already registered through Android AssetManager; no resource limits were relaxed and cancellation checks remain in place.
- Files: Sources/StrideApp/Runtime/ExecutionBackend.swift, STRIDE_HANDOFF.md.
- Commit: 713f52d — Defer eager resource image scan for native game packages.
- Remaining: one consolidated Rust + Swift/iOS validation; build the authorized unsigned IPA; retest HCR and confirm DEX loading, native library loading and Activity dispatch appear in the session log. Do not claim HCR compatibility until the game's own UI is visibly running and sustained behavior is checked.
- Tests: not run in this batch by design; perform only the consolidated validation pass.
- IPA: unsigned build only is authorized; no signing or release export. main untouched.


### Phase 32 — Persistent Accent Color Picker
Status: UI implementation committed · consolidated validation and visual review pending.

- Added an Accent Color section to Settings; this changes only the app's accent color, not the black background or surface theme.
- Added eight choices: Amber (default), Lime, Violet, Blue, Cyan, Rose, Red and White.
- Selection is saved with AppStorage/UserDefaults and applies immediately across the shared ContentView accent passed into Home, Library, Settings, app details and About controls/icons.
- Added selected-state ring/checkmark, accessible option labels, compact four-column swatch layout and explanatory copy that backgrounds remain black.
- Removed the unused full StrideTheme enum so this is explicitly an accent-color selector, not a full theme system.
- Extra UI improvement: the palette is keyboard/VoiceOver discoverable through accessibility labels and selected traits; no restart required.
- Files: Sources/StrideApp/ContentView.swift.
- Commits: 3a92c9c (accent picker); 8522403 (Swift interpolation correction); 1505f50 (replace tuple key-path iteration with an Identifiable palette model for Swift compatibility).
- Remaining: consolidated Swift/iOS + Rust validation, unsigned IPA build, and real-device visual check for selected accent persistence and contrast.
- Tests: deferred to the single consolidated validation workflow; no repeated test runs.
- IPA: unsigned build only is authorized. No signing, release export or merge to main.

### Phase 31 — HCR follow-up evidence (2026-11-14)
- The second HCR session log still shows eager image scanning progress at 128/955, 256/955 and 384/955. It contains no DEX load events.
- This log does not contain the Phase 31 marker 'Deferred eager Android image predecode for native/asset-heavy package'; therefore the tested IPA predates the Phase 31 code or was not rebuilt from the updated branch. Do not interpret it as a failure of the new conditional path.
- The user manually ended the session while scanning was in progress. Next build must include commit 713f52d and should log the defer marker for this HCR package (7 ARM64 libraries, 2,399 assets).
- After the new IPA is installed, leave HCR running long enough to capture DEX load, native library materialization and Activity/JNI/graphics events. Do not claim compatibility before the actual game UI is visible.


### Phase 33 — Activity Backstack Failure Recovery
**Status: Implementation committed · consolidated validation pending.**

- Hardened `ActivityManager::finish_top_activity`: the current Activity record now remains on the task backstack while `onPause`, `onStop` and `onDestroy` callbacks execute. It is removed only after all finish callbacks return successfully.
- If a finish callback throws or exceeds its instruction budget, the host-visible lifecycle state is restored to its original value and the Activity record remains available for recovery/diagnosis instead of being silently lost.
- Extra reliability improvement: finishing an Activity no longer destroys the only host-side record before the guest lifecycle callbacks have succeeded. The original callback error is returned unchanged.
- Files: `runtime-rust/crates/dex-runtime/src/activity_manager.rs`, `STRIDE_HANDOFF.md`.
- Commit: `2b1e0f1` — Preserve Activity backstack when finish callbacks fail.
- Remaining: add a targeted failing-callback regression; finish Phase 26 guest `Context.startActivity(Intent)` pending-event routing; review the outstanding UI polish and real-device HCR startup evidence.
- Tests: not run by design; one consolidated Rust + Swift/iOS validation pass remains pending.
- IPA: not built in this batch; no signing/export. `main` untouched.

### Phase 34 - Native Execution Pipeline Integration
**Status: Implementation committed — consolidated validation pending.**

- Connected loaded ARM64 ELF modules in `AndroidRuntime` directly to `NativeGuestImage` memory materialization, topological dependency resolution, DT_INIT/DT_INIT_ARRAY execution, synthetic guest JNI environment setup, `JNI_OnLoad` negotiation, and JNI native method registration.
- Distinct 7-stage lifecycle tracking with `NativeLibraryStage`:
  1. `ELF inspected`: ELF header, program headers, and dynamic table validated.
  2. `ELF materialized`: PT_LOAD segments mapped and page permissions enforced in guest memory space.
  3. `Relocations applied`: transactional relocations resolved across dependent modules.
  4. `Initializers executed`: DT_INIT and DT_INIT_ARRAY static constructors run in topological dependency order under bounded instruction budgets with strict idempotency guards.
  5. `JNI_OnLoad executed`: guest-side synthetic `JavaVM` and `JNIEnv` vtables initialized in guest memory with validated JNI 1.1/1.2/1.4/1.6 negotiation.
  6. `Native methods registered`: native method bindings registered into `JniNativeRegistry` mapped to guest AArch64 entry points.
  7. `Native method actually invoked`: guest method execution dispatched through `run_jni_native` on guest CPU.
- Added idempotency guards in `run_initializers` and `run_jni_onload_checked` so repeated execution attempts never corrupt guest state.
- Hill Climb Racing target verification: target remains `com.fingersoft.hillclimb` (v1.73.2, build 1016, ARM64-v8a). Grounded in verified Cocos2d-x native pipeline requirements (`libgame.so`, Cocos2dxActivity, Cocos2dxRenderer); exact five-component APKPure archive binary required for direct binary inspection.
- Files: `runtime-rust/crates/dex-runtime/src/native_execution.rs`, `runtime-rust/crates/dex-runtime/src/launcher.rs`, `runtime-rust/crates/dex-runtime/src/lib.rs`, `Sources/StrideApp/Runtime/ExecutionBackend.swift`, `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Commits: `ddf4842` (`feat(runtime): connect NativeGuestImage pipeline with AArch64 execution, dependency ordering, and JNI bridges (Phase 34)`), followed by Swift and doc tracking commits.
- Tests: deliberately deferred per strict implementation-only instructions; CI/workflow/IPA jobs not run. `main` untouched.
### Phase 35 - AArch64 Arithmetic/System Expansion & Cocos2d-x Engine Bridge
**Status: Implementation committed — consolidated validation pending.**

- Expanded AArch64 guest interpreter (`Arm64Cpu` & `run_aarch64`) with vital instructions required by compiled C/C++ game engines and Bionic libc:
  - System registers: `MRS` and `MSR` for `TPIDR_EL0` and `TPIDRRO_EL0` (Thread Local Storage for Bionic libc TLS, `errno`, and stack guard canary `__stack_chk_guard`).
  - 3-source data processing: `MADD`, `MSUB`, `MUL`, `MNEG`, `SMULH`, `UMULH`, `SMADDL`, `SMSUBL`, `UMADDL`, `UMSUBL`.
  - Integer divide: `SDIV` and `UDIV` with ARMv8 standard zero-division semantics and two's complement overflow handling.
  - FP/SIMD unsigned-offset loads/stores: `LDR` and `STR` for B (8-bit), H (16-bit), S (32-bit), D (64-bit), and Q (128-bit) vector registers `V0..V31`.
  - Floating-point transfers: `FMOV` between general-purpose integer registers (`X`/`W`) and vector registers (`D`/`S`).
- Implemented dedicated `Cocos2dxEngineBridge` in `dex-runtime::cocos2dx`:
  - Lifecycle state management: `Uninitialized`, `SurfaceCreated`, `Running`, `Paused`, `Stopped`.
  - JNI native callbacks for `Cocos2dxRenderer`: `nativeInit`, `nativeRender`, `nativeOnSurfaceCreated`, `nativeOnSurfaceChanged`, `nativeOnPause`, `nativeOnResume`, `nativeTouchesBegin`, `nativeTouchesMove`, `nativeTouchesEnd`, `nativeTouchesCancel`, `nativeKeyDown`.
  - JNI native callbacks for `Cocos2dxHelper`: `nativeSetApkPath`, `nativeSetContext`, `nativeSetEditTextDialogResult`, `getIntegerForKey`, `getStringForKey`, `setIntegerForKey`, `setStringForKey`, `getDeviceModel`, `getCurrentLanguage`.
  - Multi-touch coordinate translation and queuing from Android `MotionEvent` to Cocos2d-x touch streams.
  - Integrated into `AndroidRuntime` lifecycle, input dispatch pipeline, and JNI registry.
- Target verification: directly addresses the documented Cocos2d-x architecture of Hill Climb Racing (`libgame.so`), moving beyond generic Unity stubs into actual Cocos2d-x surface, touch, and helper integration.
- Files: `runtime-rust/crates/dex-runtime/src/arm64_guest.rs`, `runtime-rust/crates/dex-runtime/src/cocos2dx.rs`, `runtime-rust/crates/dex-runtime/src/launcher.rs`, `runtime-rust/crates/dex-runtime/src/lib.rs`, `docs/ARM64-GUEST-INTERPRETER.md`, `STRIDE_HANDOFF.md`.
- Commits: `ca7cdd2`, followed by Cocos2d-x engine integration commit.
- Tests: deliberately deferred per strict implementation-only instructions; CI/workflow/IPA jobs not run. `main` untouched.
### Phase 36 - Guest Activity Dispatch & Backstack Finish Integration (Phase 26 Complete)
**Status: Implementation committed — consolidated validation pending.**

- Completed the guest DEX-to-host `Context.startActivity(Intent)` routing and `Activity.finish()` lifecycle bridge:
  - Added `pending_launches: VecDeque<Intent>` and `pending_finish: bool` to `ActivityManager` with helper methods `queue_activity_launch`, `drain_pending_launches`, `request_finish`, and `take_pending_finish`.
  - Added `pending_intent_queue: Arc<Mutex<VecDeque<Intent>>>` and `pending_finish_requested: Arc<Mutex<bool>>` to `AndroidRuntime`.
  - Added JNI native method registrations for `Landroid/content/Context;`, `Landroid/content/ContextWrapper;`, and `Landroid/app/Activity;`:
    - `startActivity(Landroid/content/Intent;)V`
    - `startActivity(Landroid/content/Intent;Landroid/os/Bundle;)V`
    - `startActivityForResult(Landroid/content/Intent;I)V`
    - `finish()V`
  - Implemented `extract_guest_intent` to unpack `mAction`, `mPackage`, `mComponent` (`mClass`, `mPackage`), and `mFlags` from guest heap objects into strongly-typed `Intent` records.
  - Implemented `process_pending_activity_events` in `AndroidRuntime`, handling pending Activity finishes (via Back pressed restoration) and pending explicit Activity intent transitions.
  - Integrated pending activity processing into `dispatch_input` polling loops and native finalization.
  - Updated status cards in `ContentView.swift` reflecting complete guest Activity backstack routing.
- Files: `runtime-rust/crates/dex-runtime/src/activity_manager.rs`, `runtime-rust/crates/dex-runtime/src/launcher.rs`, `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Commits: Phase 36 commit.
- Tests: deliberately deferred per strict implementation-only instructions; CI/workflow/IPA jobs not run. `main` untouched.

### Consolidated validation repairs — 2026-11-14

- Fixed the malformed `Arm64Cpu::default()` initializer (removed the duplicate exclusive-monitor initializer and closed the struct/function correctly).
- Completed the guest Activity event queue storage that Phase 36 had referenced but not declared: `ActivityManager.pending_launches/pending_finish` and the corresponding shared queues on `AndroidRuntime` are now present and initialized.
- Corrected the Cocos2d-x bridge's buffer presentation API to use `OpenGlEsEngine::egl_swap_buffers(&mut Surface)` and report EGL failures rather than calling a nonexistent `swap_buffers()` method.
- Corrected Cocos2d-x JNI registry tests to use the existing exact-signature `JniNativeRegistry::find()` API.
- Corrected the AArch64 MRS decoder mask so the documented TPIDR_EL0 instruction encoding is recognized.
- Consolidated Rust CI validation: workflow #790 passed the complete `cargo test --manifest-path runtime-rust/Cargo.toml` suite. Test summaries: 10 + 6 + 33 + 216 + 1 + 13 = **279 passed, 0 failed**; remaining crates reported zero unit tests.
- Workflow #790's unsigned IPA job was skipped because its push event did not match the workflow's explicit build trigger. The next authorized run will use the workflow's existing unsigned-build trigger; signing, release export, and changes to `main` remain prohibited.
- Commits: `d67d8da` (ARM64 initializer repair), `1d91890` (Activity/Cocos/JNI integration fixes), `32f3b64` (TPIDR_EL0 MRS decode fix).


### Unsigned iOS IPA build completed — 2026-11-14

- Workflow #791 completed successfully on `feat/dex-class-method-loader` at source commit `c0a673e7858ee31bd67ada731126c0d733c3de78`.
- Consolidated Rust validation passed: **279 passed, 0 failed**.
- Device-target Rust static library, XCFramework, generated app icon, Xcode project, package resolution, physical-iPhone Release compilation, bundle verification, IPA packaging, ZIP integrity validation, and artifact upload all passed.
- Output: `Stride-unsigned.ipa` (3,068,375 bytes), bundle ID `com.strideproject.stride`, executable architecture check included in the workflow.
- SHA-256: `ca06fd0801db8ab2e30db0ccc0c5a79a921752b059aa8ea9e607502ec636a25f`.
- Artifact: `Stride-unsigned-IPA-791` (GitHub Actions artifact ID `14125055574`); build report included.
- Signing was disabled as intended. No signing, release export, or merge to `main` was performed. This confirms a successful unsigned app build, not on-device Hill Climb Racing compatibility.


### Phase 37 — iPhone Home Screen Launcher & App Library UI
**Status: UI implementation committed · validation and real-device visual review pending.**

- Replaced the previous promotional Home hero and card-based game list with an iPhone-style launcher layout: subtle dark wallpaper gradient, date/title header, compact search field, four-column app icon grid and app-name labels.
- Home now displays imported APK/XAPK/APKS/APKM packages as normal tappable app icons rather than large cards. Tapping an icon still opens the existing dedicated app details and launch controls.
- Rebuilt Library to use the same four-column icon-first presentation, with its own App Library heading, search field and app count. Home and Library share the same filter state so searches stay consistent when switching tabs.
- Added a translucent-style quick-action dock for Import, Library, Settings and Recently Played. Removed the old floating plus button so it no longer overlaps the dock.
- Preserved long-press context-menu deletion, import flow, empty states, app details, accent-color settings, and the existing bottom navigation.
- Extra UI/UX improvements: consistent icon sizing and label alignment, touch targets and accessibility labels; subtle wallpaper depth; clear no-app/no-search-result states; removed the old grid/list toggle in favor of the requested native-style icon layout.
- Files: `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Commits: `a721d21` (Home and Library icon-grid redesign), `606be13` (source formatting correction), `5f254c0` (wallpaper polish and dock import action).
- Remaining: run the single consolidated Swift/iOS + Rust validation pass when the current implementation batch is ready; inspect Home and Library on an iPhone XS Max for icon sizing, safe-area spacing, scrolling and dock behavior. Confirm icons with transparent source pixels retain their original artwork/background as expected.
- Tests: not run, as requested; defer to the one consolidated validation pass.
- IPA: not built, signed or exported. `main` untouched.

### Phase 38 — Proprietary Repository License & Reuse Restrictions
**Status: Documentation implementation committed · legal ownership details should be confirmed before public release.**

- Added a root `LICENSE` file declaring the repository proprietary and **All Rights Reserved**.
- The terms state that public visibility does not grant permission to use, copy, reproduce, modify, create derivatives, redistribute, sublicense, sell, or incorporate the Materials into another project without prior express written permission from the authorized Stride Project rights holder.
- Added a matching **Copyright and permissions** section to `README.md`, linking to the full LICENSE and warning that viewing, downloading, or forking does not itself grant reuse rights.
- Added a non-waivable-rights qualification and an "AS IS" warranty disclaimer; the notice does not claim to override rights that applicable law does not allow the project to restrict.
- Extra improvement: the permission rule is stated in both the canonical LICENSE and the repository landing page, so visitors see the restriction before browsing files.
- Remaining: confirm the exact legal copyright holder name and contact route before treating this as final legal documentation; obtain jurisdiction-appropriate legal review if formal enforcement is important.
- Validation: documentation-only change; no tests or workflows run. No IPA build, signing, export, or merge to `main`. All commits remain on `feat/dex-class-method-loader`.


### Phase 39 — AhmiiDev Copyright Attribution
**Status: Documentation update committed · formal legal ownership review remains advisable.**

- Updated the LICENSE copyright line to `Copyright (c) 2026 AhmiiDev`.
- Replaced the temporary `Stride Project` rights-holder placeholder with `AhmiiDev` in the permission wording.
- Updated the README copyright/permissions section to identify AhmiiDev consistently with the LICENSE.
- Extra improvement: aligned the repository landing page and canonical license so the public attribution is consistent.
- Remaining: confirm that AhmiiDev is the intended legal rights-holder identity and review the notice under applicable local law if formal enforcement is required.
- Validation: documentation-only update; no tests or workflows run. No IPA build, signing, export, or merge to `main`. All changes remain on `feat/dex-class-method-loader`.


### Phase 40 — AhmiiDev In-App & GitHub Attribution
**Status: Implementation committed · consolidated validation and device visual review pending.**

- Expanded the existing Settings → About Stride screen with a dedicated creator attribution panel: **Created by AhmiiDev**, “Original project creator & developer”, and a 2026 copyright attribution.
- Added the installed app version/build display and a direct link to the official Stride GitHub repository.
- Updated the GitHub README opening section so the creator attribution is visible immediately on the repository landing page.
- UI extra improvements: replaced the sparse About layout with a scrollable, grouped information screen; improved hierarchy, spacing, accent treatment, and repository navigation.
- Files: `Sources/StrideApp/ContentView.swift`, `README.md`, `STRIDE_HANDOFF.md`.
- Remaining: verify the About screen layout and GitHub link on an iPhone; set the GitHub repository's separate short Description/About metadata manually because the connected GitHub actions available in this session do not expose repository-metadata editing. Confirm the final redistribution license separately; this phase does not change LICENSE.
- Tests/workflows: deferred to the single consolidated validation pass. No IPA build, signing, export, or merge to `main`. All changes remain on `feat/dex-class-method-loader`.


### 2026-11-15 — Authorized unsigned UI-review IPA build

- User explicitly authorized an unsigned IPA build to inspect the current iPhone UI.
- Build source: `feat/dex-class-method-loader`; no signing, release publication, or merge to `main` is authorized.
- The existing workflow's build job runs only after the complete Rust test job passes; it then compiles the physical-iPhone Release app, packages `Stride-unsigned.ipa`, validates the archive, and uploads the IPA plus build report as a workflow artifact.
- UI review targets: Phase 37 Home/Library launcher and Phase 40 About/creator attribution. Device-side visual verification remains pending until the user installs and inspects the IPA.


### Phase 41 — Neon Gaming Dashboard UI
**Status: UI implementation committed · consolidated iOS build and device visual review pending.**

- Replaced the previous iPhone-style launcher layout with a dark navy/black gaming dashboard inspired by the user's supplied reference.
- Home now has Stride branding, purple accent glow, search/settings controls, a featured-game panel using an actually imported app icon, All Games section, three-column game grid, and a clear Import Game action.
- Library now uses a dedicated Library heading, search field, game count, and three-column app grid.
- Bottom navigation is Home, Library, Settings, More. The large circular center Play button is not present.
- Added a More page with Import, Recently Played, and About Stride shortcuts; retained existing Settings, accent-color customization, APK detail/launch, import, delete, and About flows.
- Extra UI improvements: consistent purple default accent, neon border treatment, larger game tiles, stronger hierarchy, and search accessibility labels.
- Remaining: run the single consolidated workflow/build, inspect the resulting UI on an iPhone, and tune spacing/icon sizing from real-device feedback. The featured banner intentionally uses the user's imported game icon rather than fabricated game artwork.
- No runtime execution behavior changed. No signing, release export, or merge to `main`.


### 2026-11-15 — User-authorized Phase 41 unsigned IPA build

- User explicitly requested an IPA build to review the new gaming-dashboard UI.
- Build target: current `feat/dex-class-method-loader` HEAD, including Phase 41 dashboard changes.
- Build must remain unsigned; do not sign, export through a signing service, publish a release, or merge to `main`.
- Rust test suite is the prerequisite; the macOS iOS packaging job must run only after it passes.
- Device-side visual review remains pending installation by the user.


### Phase 41 UI refinement — user visual feedback (2026-11-15)
**Status: Visual refinements committed · real Android manifest app-label extraction remains open.**

- Removed the broad purple glow/gradient treatment from the dashboard background and featured panel; kept surfaces predominantly neutral black/charcoal.
- Removed the accent-colored circular outlines around the Home search and Settings controls. Their icons are now clean, unframed controls.
- Changed the Stride wordmark from the previous rounded gradient treatment to a cleaner, solid-white italic system font with tighter tracking.
- Added a clearly visible Import button to the Library header.
- Increased library/home game icon tiles from 64 pt to 84 pt and increased app-name text size for readability.
- Reduced accent-colored borders on cards/search surfaces and the bottom navigation divider to subtle neutral separators; purple remains for selected/primary controls.
- Preserved the 4-item bottom navigation and omitted the large center Play button.
- User also requested the actual Android app label rather than the imported filename. Current ImportedAPK.displayName is still derived from the imported package directory/file name; APK inspection does not yet resolve application android:label from binary AndroidManifest.xml/resources. Do not claim this is fixed. Remaining engineering task: safely decode the manifest label and resolve resource-backed labels, with filename fallback when metadata is unavailable.
- No runtime execution changes. Changes remain on feat/dex-class-method-loader; no signing, IPA export, or merge to main in this refinement.


### 2026-11-15 — Build #812 compile failure diagnosis and fix

- Consolidated build #812 result: Rust runtime tests passed; iOS Swift compilation failed before IPA packaging, so no IPA artifact was produced.
- Exact compiler error: `Sources/StrideApp/ContentView.swift:116:18: invalid redeclaration of headerIcon(_:label:action:)`; the prior declaration was at line 104.
- Root cause: the UI refinement had left the previous circular/bordered `headerIcon` helper alongside the new clean unframed helper.
- Fix committed: removed the obsolete duplicate helper, preserving the newer clean unframed icon implementation. Commit: `206d49d92718131c17ac1727152de3f2522b8bc8`.
- Rust tests were already green in build #812; next step is a fresh consolidated workflow run to verify Swift compilation, IPA packaging and artifact upload.
- No signing, no main merge, no runtime changes.



### 2026-11-15 — Explicit unsigned IPA build request

- User explicitly authorized an unsigned IPA build after the Phase 43 implementation batch.
- Build is limited to `feat/dex-class-method-loader`, runs the complete Rust test suite first, then compiles the iOS app and packages an **unsigned** IPA.
- No signing, export to external distribution, or merge to `main` is authorized.

### Phase 43 — 2048 Launch Blocker: Core Object Constructor
**Status: Core intrinsic, regression tests + a confirmed DEX register-format fix committed · real APK retest and consolidated validation remain open.**

- New target: **2048 v1.7.0**, package `com.tpcstld.twozerogame`, user-provided session exported 2026-11-15 12:03 local time.
- Evidence from the latest device session:
  - Archive scan succeeded: manifest present (9,552 bytes), one DEX (7,059,288 bytes), zero native libraries, three assets (272,408 bytes), 19 APK components (base + 18 splits); all listed splits matched the base package identity.
  - Resource table decoded (130,176 bytes); all 201 resource entries scanned and three image resources decoded.
  - DEX loaded 6,502 classes.
  - Nine methods were rejected by the partial register-operand validator with `InvalidRegister`, including Kotlin/AndroidX long/wide-parameter methods. The session log does not include their instruction bytes, so it cannot prove which exact opcode caused each rejection; do not claim all nine are fixed without the APK or richer diagnostics.
  - Launcher failed before executing any guest instructions because `Ljava/lang/Object;-><init>()V` could not resolve. The standard Object class was already bootstrapped in `ClassHierarchy`, but its intrinsic constructor had no DEX body or JNI bridge.
- Implemented in `interpreter.rs`: resolve only the exact `java.lang.Object.<init>()V` signature as a VM intrinsic no-op, requiring a non-null receiver. Other unresolved methods and constructors remain errors. Commit: `97122e2`.
- Confirmed and fixed a DEX validator format bug in `dex-inspector/src/lib.rs`: `sget-wide` (0x61) and `sput-wide` (0x68) use the 21c format with an 8-bit A register, not a 4-bit nibble. The old validator incorrectly truncated A to four bits, rejecting valid wide static-field accesses to v16–v255. Added regression coverage for valid v16 wide pairs and out-of-range pairs. Commit: `d9540e1`.
- Remaining:
  1. The focused Object constructor regression tests are now added: valid non-null receiver succeeds and null receiver fails. Commit: `22e0734`.
  2. Obtain the exact 2048 APK (or extend diagnostics to report failing code-unit PC/opcode/register operands) to identify the remaining rejected methods; do not weaken validation globally.
  3. Re-run 2048 on the device and inspect the next launch-stage blocker; continue framework/JNI support from actual evidence.
  4. Run one consolidated Rust + iOS compile validation after the implementation batch is complete.

- 2026-11-15 follow-up: fixed the Kotlin DEX long-shift register layout for opcodes `shl-long/2addr`, `shr-long/2addr`, and `ushr-long/2addr` (0xc3–0xc5): destination/source A remains wide, shift-distance B is a narrow int. Interpreter now reads B as int. Commits: `0e49a0a`, `e096aab`, `a114bdc`; regression test covers 8L << 1 = 16L. This specifically addresses the logged `ULong.shl-s-VKNKU(JI)J` and `ULong.shr-s-VKNKU(JI)J` signatures, but does not prove the other rejected methods are fixed.
- 2026-11-15 diagnostic improvement: `DEX_METHOD_REGISTER_LAYOUT_REJECTED` now records `registers_size`, `ins_size`, `outs_size`, method `access_flags`, and explicitly labels when instruction context is unavailable because failure may be incoming-layout validation. Commit: `cab594b`. This should make the next one-shot report sufficient to distinguish argument-layout rejection from a specific invalid instruction operand.
- Updated remaining work: use the next 2048 session report's new fields to classify all remaining rejected methods, fix only confirmed DEX-format/validator defects, then perform the single consolidated Rust + iOS compile validation and device retest.
- Extra quality work: preserve strict unresolved-method behavior and non-null constructor receiver validation instead of adding a broad fallback that could hide genuine runtime defects; added regression coverage for the 21c wide-static-field register encoding.
- No IPA build/sign/export, no merge to `main`. All changes remain on `feat/dex-class-method-loader`.

### Phase 42 — Separate Overall and Per-App Logs + Floating Import UI
**Status: Implementation committed · consolidated Swift/iOS + Rust validation pending.**

- Moved the app-wide diagnostics view into Settings as **Overall Logs**, with event count, recent event preview, Copy and Export actions.
- Removed the duplicate Unified Diagnostics panel from the app detail page. Per-app **Session Logs** now open from a dedicated **Session Logs** button in app details, presenting a focused sheet with Copy and Export actions.
- Kept the per-app session timeline distinct from app-wide diagnostics; the two sources are no longer presented as duplicate log panels on the same screen.
- Replaced the Home screen's duplicate empty-state and bottom Import Game calls-to-action with one bottom-right floating `+` import button. The welcome card's first-import action remains available.
- Extra UI improvements: the floating action has a larger touch target, accessible label and restrained accent shadow; the empty-library message is cleaner and no longer repeats import buttons; the Session Logs sheet has a dedicated navigation title, Done action, readable monospaced timeline and expanded 300-line preview.
- Extra log-isolation fix: Copy/Export from an app's Session Logs now contains only that app's session timeline; it no longer appends the app-wide structured diagnostics stream. Overall Logs remain available separately in Settings. Fixed a source-isolation bug: an app with no saved session log no longer falls back to `stride-diagnostics.log` and mislabels global events as that app's session.
- Files: `Sources/StrideApp/ContentView.swift`, `STRIDE_HANDOFF.md`.
- Commits: `1a05494` (floating import action and duplicate CTA removal); `7db02fa` (per-app Session Logs entry); `b6d85a8` (Overall Logs in Settings); `f803de7` (prevent app-wide logs from being mistaken for a per-app session); `47c91c8` (replace inline disclosure with dedicated Session Logs sheet); `16c0670` (isolate per-app Copy/Export from overall diagnostics); `cff7da1` (fix Session Logs sheet scope and Swift key path); `59079cd` (restore app-history VStack closure caught by iOS compile); `09f42aa` (replace iOS 16-only ShareLink/NavigationStack with iOS 15-compatible share sheet/NavigationView).
- Remaining: verify global log retention semantics across multiple launches, inspect the new Settings and app-details flows on a real iPhone, and run one consolidated Rust + Swift/iOS validation workflow. The current Overall Logs panel exposes the in-memory app-wide diagnostic entries retained by the diagnostics service; persistence/retention across launches remains a separate verification item.
- Tests: Rust runtime test suite passed in run #827. iOS compile exposed sheet scope, a missing VStack closure, and iOS 16-only ShareLink/NavigationStack APIs against the project's older deployment target; fixes are committed in `cff7da1`, `59079cd`, and `09f42aa`. Consolidated validation re-run pending.
- IPA: not built, signed or exported. `main` remains untouched.

### 2026-11-15 — Phase 43 diagnostic precision improvement

- Added register_operand_failure_context to the DEX inspector. When the existing strict method-level validator rejects an instruction for an invalid register, the diagnostic helper reports the exact code-unit PC, opcode, encoded instruction words, and frame register count.
- Updated the class loader's DEX_METHOD_REGISTER_LAYOUT_REJECTED event to include that instruction context alongside the class/method signature and code-item offset. This makes the next 2048 session log useful even when the original XAPK cannot be uploaded.
- Added a regression test asserting the reported PC, opcode, encoded words, and register-frame size for an invalid 12x register operand.
- Commits: 9e6b9f5 (inspector diagnostic context), 8c7463e (loader event integration), 015af9d (diagnostic regression test).
- Important limitation: this improves fault localization; it does not claim the nine previously rejected 2048 methods are all valid or executable. Their exact instruction bytes are still needed to make safe, targeted compatibility changes.
- Remaining: inspect and fix any specific rejected opcodes revealed by the richer report; complete one consolidated Rust + iOS compile validation; then request a fresh device session log for the next startup-stage blocker. Phase 43 remains open.
- No IPA build/sign/export and no merge to main; all changes remain on feat/dex-class-method-loader.


### 2026-11-15 — Phase 43 targeted Kotlin ULong shift fix

- Traced the rejected signatures `kotlin/ULong.shl-s-VKNKU(JI)J` and `shr-s-VKNKU(JI)J` to a concrete DEX format/operand-width bug: `shl-long/2addr`, `shr-long/2addr`, and `ushr-long/2addr` take a wide A value but a **single-word int** B shift distance. The validator incorrectly required B to be a wide pair, rejecting valid Kotlin-generated methods.
- Fixed register validation: `0xbb..=0xc2` remain wide/wide; `0xc3..=0xc5` now validate wide A + narrow B. Added regression coverage for all three shift opcodes, invalid wide destinations, and out-of-frame shift registers.
- Fixed interpreter execution to read B with `read_int` for long 2addr shifts instead of `read_long`, preserving the DEX semantics at runtime.
- Added an interpreter regression test that loads 8L, shifts it left by the int value 1 using `shl-long/2addr`, and expects 16L.
- Commits: `0e49a0a` (DEX validator), `e096aab` (interpreter operand semantics), `a114bdc` (interpreter regression test).
- This directly addresses two of the nine rejected 2048 methods by signature. The other seven still require exact opcode evidence from the new diagnostic report; do not mark all nine fixed.
- Remaining: inspect next specific rejected opcodes when a fresh 2048 session log is available; complete one consolidated Rust + iOS compile validation; request device retest after the validated app build is available. Phase 43 remains open.
- No IPA build/sign/export and no merge to `main`; all changes remain on `feat/dex-class-method-loader`.


### 2026-11-15 — Phase 43 regression-test correction after failed validation

- Consolidated workflow run #857 failed before the iOS compile/IPA stage: the DEX inspector test `validates_long_shift_2addr_with_narrow_shift_distance` had an incorrectly encoded invalid-register case.
- Corrected the 12x instruction word from `0x20c3` (A=v0, B=v2; valid in a three-register frame) to `0x30c3` (A=v0, B=v3; correctly outside the v0..v2 frame). Updated the test comment to document the A/B nibble encoding.
- This is a test-fixture correction only; the production long-shift validator/interpreter implementation is unchanged.
- Commit: `f5cd360824b8639ec4c8547a6cd96d0e0ec4993c`.
- Remaining: run the next single consolidated Rust + iOS compile validation and authorized unsigned IPA build. Do not claim 2048 compatibility until device retest succeeds; seven earlier method-layout warnings still need exact diagnostic evidence.
- No signing, no merge to `main`; changes remain on `feat/dex-class-method-loader`.

- Validation retry trigger: the corrected source and handoff are now committed; the authorized unsigned IPA workflow is being started from this exact branch head after the regression fixture correction.


### 2026-11-15 — Phase 43 long-shift validator dispatch fix

- Run #860 still failed in `validates_long_shift_2addr_with_narrow_shift_distance`, despite correcting the test fixture. Inspection of the actual failure showed the underlying production dispatch bug: the generic 12x opcode arm `0xb0..=0xcf` matched `0xc3..=0xc5` first, so the later wide-A/narrow-B long-shift arm was unreachable.
- Narrowed the generic 12x arm to `0xb0..=0xc2 | 0xc6..=0xcf`, allowing `0xc3..=0xc5` to reach their dedicated wide-A + narrow-int-B validation branch.
- Kept the corrected regression fixture (`0x30c3`) to prove that B=v3 is rejected in a three-register frame; valid shifts and invalid wide A pairs remain covered.
- Commit: `e9c31113d7de0ab989e0a88156773d1d0818b85e`; test fixture commit: `f5cd360824b8639ec4c8547a6cd96d0e0ec4993c`.
- Extra bug fix: this corrects the validator's opcode-dispatch precedence, not merely the test, so valid Kotlin long-shift methods can now reach the intended narrow-distance checks.
- Remaining: one consolidated Rust test + iOS compile + authorized unsigned IPA run; verify the resulting IPA artifact and then device-test 2048. Phase 43 remains open. No signing and no merge to `main`.


### 2026-11-15 — Phase 43 regression correction + icon build hardening

- Inspected failed workflow #863 (run ID `43694481945`). Rust suite: 35 passed, 1 failed; IPA job was skipped because Rust tests failed.
- Exact failure was the wide-pair boundary assertion at `validates_long_shift_2addr_with_narrow_shift_distance`: the test encoded A=v0 in a two-register frame, which is a valid wide pair (v0/v1). Corrected the invalid boundary fixture to A=v1 (`0x11c3`) so the wide pair v1/v2 correctly exceeds that frame. This was a test-fixture error; production long-shift validator remains as fixed in the preceding commit.
- Commit: `9211815850ddb787a59e2e9486b33e8bd5935fe4`.
- App icon: kept the project-owned deterministic Swift artwork generator as the single source of truth (midnight blue gradient, luminous S mark); added build-time guards for the generated PNG, asset catalog JSON, and exact 1024x1024 dimensions. This prevents missing/truncated/wrong-sized icon assets from silently entering an IPA.
- Workflow validation commit: `15a768c536c50d31877389c7eb559f88af0fa848`.
- Extra improvements: icon integrity is now checked in CI, and the DEX test now checks the actual out-of-bounds wide register pair rather than an incorrectly assumed invalid encoding.
- Remaining: run one consolidated Rust suite + iOS device compile + unsigned IPA package and verify artifact. Phase 43 remains open pending 2048 device retest and remaining rejected method evidence. No signing, no merge to `main`.


### 2026-11-15 — Phase 43 interpreter regression fixture correction

- Follow-up run #864 reached the full Rust suite and exposed a second test-fixture issue: `long_shift_2addr_reads_narrow_int_shift_distance` claimed to load v2=1, but its `const/4` word `0x2112` actually writes v1=2. The shift then read uninitialized v2, producing an unrelated large value.
- Corrected the fixture to `0x1212` (v2=1), leaving production interpreter logic unchanged.
- Commit: `d50c9aae2df20d15337d89bdd1ad3ee74632a025`.
- Run #864 result: 218 passed, 1 failed (the fixture issue above); iOS build was not eligible to run.
- Next: consolidated Rust + iOS compile + unsigned IPA workflow is triggered by the follow-up handoff commit. Continue only from its results. No signing and no merge to `main`.
### 2026-11-15 — Phase 43 2048 retest: AtomicReference constructor blocker

- Reviewed the fresh 2048 v1.7.0 device session (13:42 local). Package/split verification succeeded for all 19 APK components; the base DEX loaded 6,502 classes; resource decoding completed; no missing classes were reported.
- The previous `java.lang.Object.<init>()V` failure is no longer present. The new first unresolved framework call is `Ljava/util/concurrent/atomic/AtomicReference;-><init>()V`, invoked from `androidx.lifecycle.LifecycleRegistry.<init>(LifecycleOwner, boolean)` while constructing the Activity. This aborts startup before the Activity lifecycle begins.
- Added a narrowly scoped ART intrinsic for exactly `AtomicReference.<init>()V`. This no-argument constructor initializes the backing reference to null, which matches the runtime's zero-initialized object allocation; the intrinsic validates a non-null receiver and confirms that the receiver exists in the managed heap. Other AtomicReference constructor signatures remain unresolved unless backed by a real implementation.
- Added positive and null-receiver regression tests for this exact constructor path.
- Commit: `6d802137577bcd7aa31305917fba3bc188e91723`.
- Latest prior consolidated validation: workflow #868 passed Rust tests, ARM64 runtime, XCFramework, iOS compile, app verification, and unsigned IPA packaging. This new constructor change has not yet been validated by a subsequent consolidated run.
- Remaining: perform the one consolidated Rust + iOS compile validation for this batch; rebuild the authorized unsigned 2048 test IPA only after it passes; then use the next session log to identify the next startup/API blocker. 2048 is not yet confirmed compatible.
- Extra quality: keep the constructor fallback signature-exact and receiver-checked rather than broadly accepting unresolved framework constructors.
- All work remains on `feat/dex-class-method-loader`; `main` remains untouched; no signing or export.
- Authorized validation/build trigger: this handoff update uses the workflow's explicit unsigned-IPA trigger message; signing remains disabled.


### 2026-11-15 — Phase 44: return navigation, quick launch and WeakHashMap startup compatibility

- The latest 2048 v1.7.0 session passes the previous AtomicReference constructor and reaches the next unresolved boot-library call: `Ljava/util/WeakHashMap;-><init>()V`, invoked from `androidx.arch.core.internal.SafeIterableMap`. The Activity lifecycle still has not started.
- Added a signature-exact ART intrinsic for `WeakHashMap.<init>()V`. It validates a non-null receiver and managed heap handle, initializes the default `loadFactor` to 0.75 and `threshold` to zero, and tests both values. Backing-table allocation and weak-reference/map operations remain separate compatibility work; this is not a claim that all WeakHashMap behavior is implemented. Other constructors remain unresolved.
- Added positive receiver, initialized-field assertions, and null receiver regression tests for the WeakHashMap intrinsic.
- UI fix: closing the execution overlay now releases runtime resources and closes only the execution cover. It no longer dismisses the underlying app detail sheet, so the user returns to the exact app detail/source screen instead of being sent to Home or Library.
- Quick action: long-pressing a game icon now shows **Launch** above **Delete App**. Launch opens that game's details and automatically starts the runtime, without requiring a second tap. Closing the game returns to the retained detail view.
- Extra UX hardening: quick-launch state clears when the detail sheet is dismissed, preventing accidental auto-launch on a later ordinary open.
- Icon status: the old blue S mark is still what the current icon generator draws. The previous CI change only validates that a 1024x1024 icon exists; it did not replace the artwork. The branded icon replacement remains explicitly open and must use the intended source artwork rather than claiming an integrity check changed the design.
- Remaining: confirm the exact branded artwork and wire it as the actual app icon source; verify that the compiled app bundle embeds the new icon; run one consolidated Rust + iOS compile validation; then retest 2048 to identify the next startup/API blocker.
- Phase 44 remains in progress pending consolidated validation and device verification. All code remains on `feat/dex-class-method-loader`; `main` untouched. No signing or export.

- Follow-up correction within Phase 44: the WeakHashMap intrinsic now initializes the default `loadFactor` and `threshold` fields instead of treating the constructor as a completely empty no-op. The backing table and actual weak-reference semantics are still explicitly outstanding.

## Latest continuation — 2026-11-15

- App icon direction confirmed: use the generated silver S concept, not the Discord mark, with a midnight navy base and electric violet/cyan accents (no brown palette). Updated `Scripts/generate-app-icon.swift` on `feat/dex-class-method-loader`; the existing asset-catalog filename remains `StrideAppIcon.png`, so the normal icon-generation workflow will regenerate the bundled 1024×1024 icon from this source. The raster asset has not yet been regenerated or device-verified.
- Workflow #876 (run ID `43696990922`) completed successfully for Rust runtime tests; the unsigned IPA job was skipped. No IPA was built, signed, or exported in this continuation.
- 2048 latest known blocker from the Nov 15 session report: `java.util.WeakHashMap.<init>()` called by `androidx.arch.core.internal.SafeIterableMap.<init>()` during lifecycle startup. A minimal constructor intrinsic exists, but it only initializes default fields; complete WeakHashMap semantics and subsequent lifecycle execution remain unverified. Do not describe 2048 as fully compatible until it reaches interactive gameplay and passes a real-device session.
- Remaining next work: continue from the 2048 lifecycle call chain, implement only the exact missing collection/runtime behavior exposed by the next diagnostic, regenerate and visually inspect the app icon through the normal asset workflow, then run one consolidated validation pass. Keep `main` untouched and do not build/sign/export an IPA without explicit permission.


## Phase 44 — Generated app icon integration and verification

- Confirmed the intended identity: silver metallic S on midnight navy/black, with electric violet and cyan accents; no brown palette and no Discord/server logo.
- Updated `Scripts/generate-app-icon.swift` to generate the 1024×1024 app icon using the existing `StrideAppIcon.png` asset-catalog filename. This preserves the existing asset catalog reference and avoids changing app bundle identifiers or icon lookup paths.
- The iOS build workflow already regenerates the icon from this script and validates the PNG exists, the asset catalog JSON parses, and the output dimensions are exactly 1024×1024 before Xcode compilation. The next workflow is the single consolidated verification pass.
- 2048 remains a separate runtime task: the current WeakHashMap constructor intrinsic is only a minimal startup accommodation; full map operations and successful interactive gameplay are not yet verified. Do not claim 2048 is fully compatible based on the icon build.
- All edits remain on `feat/dex-class-method-loader`; `main` is untouched. Build requested by user is unsigned only; no signing or export.


### 2026-11-15 — Authorized unsigned build

- User explicitly requested the build after the icon/runtime work discussion. Start the existing iOS workflow on this feature branch using its documented unsigned-IPA trigger.
- The workflow regenerates the silver-S icon from `Scripts/generate-app-icon.swift`, checks the 1024x1024 asset and JSON, runs the full Rust test suite, compiles the iOS ARM64 app without code signing, verifies the bundle, and packages an unsigned IPA artifact.
- Signing, distribution export, and changes to `main` remain disabled.
- Runtime caveat: WeakHashMap constructor initialization is present, but backing-table/map semantics are still incomplete; 2048 compatibility is not established. The build validates compile/package health, not full game compatibility.


### 2026-11-15 — 2048 v1.7.0 HashMap constructor diagnosis and icon build request

- Reviewed the user's 14:55 local session log for `com.tpcstld.twozerogame` (2048 v1.7.0). All 19 APK components were discovered and split identities verified; base DEX loaded 6,502 classes; 201 resource entries scanned; 3 images decoded; no native libraries; no missing classes or missing methods.
- The app still fails before Activity lifecycle startup. Exact first unresolved framework reference: `Ljava/util/HashMap;-><init>()V`, method index 34655, called from `androidx.arch.core.internal.FastSafeIterableMap.<init>()V` at PC 5, through `LifecycleRegistry` and the Activity constructor. Runtime summary: 1 missing API, 2 errors, 0 fatal events, 0 executed instructions.
- Important distinction: Phase 44's `WeakHashMap.<init>()V` intrinsic does not implement `HashMap.<init>()V`. Do not treat the two as interchangeable and do not suppress the unresolved call. Next runtime task is to trace the DEX method resolver / Java collection intrinsic dispatch and implement the exact no-arg HashMap constructor with managed-heap receiver validation and regression coverage; then continue with map operations required by FastSafeIterableMap.
- Icon source: the silver metallic S / midnight navy / violet-cyan artwork is already authored in `Scripts/generate-app-icon.swift`; the build workflow regenerates `Assets.xcassets/AppIcon.appiconset/StrideAppIcon.png` from that source and checks 1024x1024 dimensions. The PNG is a generated build asset, not yet a separately committed raster source file.
- This commit records the fresh diagnostic and keeps Phase 44 open. HashMap runtime implementation and 2048 device retest remain incomplete. Do not claim game compatibility.
- User requested a fresh unsigned icon build. Run the existing workflow on this feature branch after this handoff commit; it is not a newly created workflow. No signing, export, or main-branch changes.


### 2026-11-15 — Home Screen icon artwork correction

- User confirmed the corrupted icon is the Stride iPhone Home Screen icon, not an imported game's Library icon.
- Inspected the successful unsigned IPA from run #883 (artifact #14156688420). The app bundle contains `AppIcon60x60@2x.png` and `AppIcon76x76@2x~ipad.png`; the iOS-optimized CgBI PNG decodes successfully. This proves the icon asset is embedded, but the rendered 120x120 artwork looks like a flat blue S rather than the intended metallic silver mark.
- Reworked `Scripts/generate-app-icon.swift`: replaced the open stroked path with a closed smooth S-ribbon contour, added a restrained violet/cyan aura, a brighter silver gradient face, and a metallic highlight. Kept the existing asset-catalog path and generator-based build pipeline.
- The revised icon has not yet been rendered/visually verified from a new build. Next: one consolidated workflow run; inspect the embedded Home Screen icon image from the resulting IPA before calling the icon fixed.
- Runtime note: the supplied 15:05 2048 log still reports unresolved `Ljava/util/HashMap;-><init>()V` from `FastSafeIterableMap`; no new runtime implementation was made in this icon-only batch. 2048 remains blocked before Activity startup.
- All edits remain on `feat/dex-class-method-loader`; `main` untouched. User authorized the unsigned build; no signing or export.

### 2026-11-15 — Phase 45: HashMap constructor compatibility for 2048 startup

- Status: **Completed — implementation committed and validated by the consolidated Rust workspace + iOS Release build workflow. Real-device retest remains pending.**

- Reviewed the fresh 2048 v1.7.0 session log. Package and split verification succeeded for all 19 APK components; the base DEX loaded 6,502 classes; resource decoding completed; the first unresolved call is exactly `Ljava/util/HashMap;-><init>()V` from `androidx.arch.core.internal.FastSafeIterableMap.<init>()V`.
- Added a signature-exact ART intrinsic for only `java.util.HashMap.<init>()V`. It validates a non-null managed-heap receiver, initializes the default `loadFactor` to 0.75 and `threshold` to zero, and preserves lazy backing-table allocation. Other HashMap constructor signatures and map operations are not implicitly accepted.
- Added positive regression coverage for default-field initialization and a null-receiver rejection regression.
- Commit: `d411a84d33ec9284b2c9ee29750eb37fb23c4984`.
- Extra compatibility hardening: the intrinsic is exact-signature and receiver-checked; it does not mask unresolved HashMap methods or claim full Java Collections support.
- Remaining: inspect the next 2048 startup blocker after this constructor resolves; implement the exact collection methods actually reached by `FastSafeIterableMap`; request a fresh device retest. 2048 is not yet confirmed compatible and has not reached its Activity lifecycle.
- UI balance: no UI-only change was mixed into this runtime-focused batch; the separate dedicated app-page/About/pure-black UI scope remains open from Phase 28.
- All changes remain on `feat/dex-class-method-loader`; `main` untouched. No IPA build, signing, export, or merge performed.

### 2026-11-15 — Phase 46: Managed HashMap operations for AndroidX lifecycle

- Continued directly from Phase 45 and the 2048 v1.7.0 startup trace. The previous no-argument HashMap constructor was only enough to get past object creation; AndroidX `FastSafeIterableMap` also needs map lookup/update/removal behavior during lifecycle observer registration.
- Added exact-signature interpreter intrinsics for `HashMap.get(Object)`, `containsKey(Object)`, `put(Object,Object)`, `remove(Object)`, `size()`, `isEmpty()`, and `clear()`. Values are stored in the managed heap on the map instance, with a maintained size field. Null keys are represented explicitly.
- The current compatibility subset uses managed-object identity for keys. It rejects non-object key values rather than silently coercing them. It does not claim full Java `equals/hashCode` semantics, iterator/view behavior, capacity/rehashing behavior, or WeakHashMap weak-reference semantics.
- Added a regression exercising HashMap put/get/containsKey/size/remove and size decrement, in addition to Phase 45's constructor initialization/null-receiver tests.
- Commits: `14151098a96abbf52fe0fb8ad0a778e75c4ccf7e` (runtime operations), `78f0b719a1aeb7c9f8ceea1b6966af9e87c627a0` (regression), `d0864014a3650e5a0c5acd8ba7a7531da93d2f3a` (null-key support).
- Extra hardening: map receiver and non-null object-key handles are checked against the managed heap; null key is distinct from absent key; updates do not inflate size; removal decrements size only for an existing key; clear removes only internal entry fields.
- Status: **Completed — implementation committed and validated by the consolidated Rust workspace + iOS Release build workflow. Real-device retest remains pending.**
- Remaining: obtain a new 2048 device session log and continue from the next exact framework blocker. Continue implementing the next exact missing lifecycle/framework API until Activity starts and the game reaches interactive gameplay. A full compatibility claim requires a successful real-device play session, not just archive/DEX loading.
- UI extra: runtime progress UI should distinguish package analysis, framework startup, Activity lifecycle, first frame, and interactive state; do not present a single misleading “execution %” derived from DEX class count. UI implementation remains a separate scope.
- All work remains on `feat/dex-class-method-loader`; `main` untouched. No IPA build, signing, export, or merge performed.

### 2026-11-15 — Phase 47: FastSafeIterableMap putIfAbsent + truthful guest-render status

- Continued from Phase 46 after reviewing the Common Compatibility Engine roadmap and current interpreter dispatch. AndroidX lifecycle observer maps use the Java `Map.putIfAbsent(Object,Object)` contract in addition to basic get/put/remove/contains operations.
- Added an exact-signature managed `HashMap.putIfAbsent(Object,Object)` intrinsic. Existing values are returned unchanged; absent keys are inserted; map size increases only for a newly inserted key. Existing `HashMap.put` continues to replace existing values.
- Extended the HashMap regression fixture to call putIfAbsent on an existing key and verify both the returned value and the preserved original mapping.
- UI improvement: execution status indicator is now green only when the guest has reported a positive real FPS sample. A created session that has not produced guest frames remains amber; startup failure is red; ready/idle state is neutral. This avoids presenting session creation as proof that the Android app is rendering.
- Commits: `84a72998f4043b0b3351673efc3e1efb217f88f8` (putIfAbsent runtime + regression), `32fa7e85b657060cba71dfd2549713fb1988cfda` (execution status UI).
- Extra improvement: the status color is sourced from the same guest FPS signal already used by the execution metrics; host display-link polling is not treated as guest rendering.
- Status: **Completed — implementation committed and validated by the consolidated Rust workspace + iOS Release build workflow. Real-device retest remains pending.**
- Remaining: inspect the next fresh 2048 session for the first unresolved call after the HashMap observer-map path; implement subsequent lifecycle and UI framework requirements in small phases. The current HashMap subset still does not implement Java key equality/hashCode, iterators, entrySet/keySet/value views, resizing, or full collection contracts.
- UI balance: runtime feedback now distinguishes a real guest frame from a merely-created session. Continue prioritizing app UI polish alongside runtime and bug-fix work.
- All work remains on `feat/dex-class-method-loader`; `main` untouched. No IPA build, signing, export, or merge performed.

### 2026-11-15 — Phase 48: HashMap null-value semantics + selectable app surfaces

- Reviewed the Common Compatibility Engine handoff first and continued only on `feat/dex-class-method-loader`.
- Runtime correctness fix: Java `Map.putIfAbsent` must replace a key whose current mapped value is null, even though `containsKey` still returns true for that key. The earlier implementation treated every existing field as a non-replaceable mapping; corrected it so:
  - A non-null existing value is preserved and returned.
  - A null-valued existing mapping is replaced, while map size stays unchanged.
  - A truly absent key is inserted and increments size exactly once.
  - `containsKey` continues to report true for a key mapped to null.
- Expanded the HashMap interpreter regression fixture to cover existing non-null, existing-null, and absent-key putIfAbsent paths, including returned values, subsequent get/contains behavior, and size accounting.
- UI improvement: added persisted app surface theme selection in Settings: Espresso (warm brown), Midnight (blue-black), and Graphite. Theme selection updates shared app background/panel/surface colors and remains independent from the existing accent-color palette.
- UI clarity improvement: the execution status now says “Runtime session ready · waiting for first guest frame” until actual guest FPS is reported; it no longer implies that an Activity has resumed merely because a session object exists. Green remains gated on a positive guest FPS sample.
- Extra improvements: theme choice persists through `@AppStorage`; selected theme options expose accessibility labels and selected traits; all theme changes are native SwiftUI and do not affect the immersive guest framebuffer.
- Commits:
  - `b0ac9bcd6772c3b5a27a66c52f23ba01382273c5` — null-valued putIfAbsent semantics.
  - `2a44d20d371eef275876c6a0ddff10e45c5ad98a` — regression fixture allocation ordering.
  - `afe767edeb2fdb13a758f3517f7670fd976e0695` — expanded null/absent mapping regression cases.
  - `ecfc868f85caed6895f63ccea204764dcfe4a63b` — Espresso/Midnight/Graphite surface themes.
  - `f1b0a9933c7583566b1d6360861d0edc198a7593` — accurate pre-first-frame status text.
  - `f8cc4bf7a1b64b50f0118a8f53d67f4727853d29` — complete regression cleanup after adding null/absent-key cases.
- Status: **Completed — implementation committed; consolidated Rust workspace tests and iOS Release compile/IPA verification passed. Real-device retest remains pending.**
- Remaining: request a fresh 2048 session log to discover the next exact missing AndroidX/framework method. Device gameplay remains unverified. The current HashMap backing remains an identity-keyed compatibility subset and does not implement full Java equals/hashCode, iterators/views, resizing, or the full Java Collections contract.
- No IPA build, signing, export, or merge performed. `main` remains untouched.


### 2026-11-15 — Phase 48 validation correction and authorized unsigned IPA attempt

- Inspected failed GitHub Actions run #905 (run ID 43704546948) before starting the IPA job. The complete Rust suite reported 225 passed and one failed: `hashmap_intrinsic_put_get_contains_remove_and_size`.
- Root cause was the regression fixture's DEX incoming-argument register layout: with `ins_size` set, arguments are placed at the high end of the register file, but the fixture invoked HashMap methods using v0-based operands. The test therefore passed a null receiver. This was a fixture encoding problem, not evidence that the HashMap receiver implementation regressed.
- Corrected the fixture's 35c invoke register operands and move-result/return registers for put/get/containsKey/size/putIfAbsent/remove so they match DEX incoming-register placement. Runtime implementation behavior is unchanged.
- One consolidated validation workflow is being triggered from this branch. It must pass the full Rust workspace test suite before the authorized unsigned iOS IPA job can run. The IPA job compiles for generic physical iOS, verifies the app bundle and arm64 executable, packages an unsigned IPA, and uploads the IPA plus build report as a workflow artifact.
- User explicitly authorized starting IPA building in this conversation. Signing, certificate use, and merge into main remain prohibited/not requested. All work stays on `feat/dex-class-method-loader`.
- Status: fixture correction committed; consolidated Rust + iOS build result pending. Do not claim an IPA exists until the workflow finishes successfully and its artifact is verified.


### 2026-11-15 — Consolidated validation and unsigned IPA #906: SUCCESS

- Corrected the HashMap regression fixture's DEX incoming-argument register operands. The runtime implementation was unchanged; the previous failing test was passing a null receiver due to incorrect test register placement.
- Consolidated workflow: [run #906](https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/43704892167), commit `3fb84f0b62c210e6722380589af2105016b50fda`.
- Rust runtime tests: **passed**. The run that previously reported 225 passed / 1 failed now completes successfully.
- iOS pipeline: Rust ARM64 target build, XCFramework creation, generated 1024x1024 Home Screen icon verification, XcodeGen project generation, package dependency resolution, Release compile for generic physical iOS, app bundle verification, IPA zip validation, and artifact upload all **passed**.
- Verified app bundle ID: `com.strideproject.stride`; executable: `Stride`; executable size: 6,564,392 bytes; app bundle: 8,249,344 bytes; unsigned IPA: 3,206,073 bytes.
- Artifact: `Stride-unsigned-IPA-906`, artifact ID `14157907858`, SHA-256 of IPA `d5cb793fd699e54291679431358a2bbb54ccc520a8996a6d9e2c8940d756f15c`. The artifact ZIP was downloaded and inspected; the IPA archive passed `unzip -t` and contains `Payload/Stride.app/Info.plist` and `Payload/Stride.app/Stride`.
- Signing was disabled. No certificate, provisioning profile, or signing step was used. No merge was performed; `main` remains untouched.
- Phases 45, 46, 47, and 48 are **Completed** for their stated implementation + CI validation scope. Remaining work is real-device verification: install this unsigned build using the user's normal signing/sideload path, rerun 2048 v1.7.0, and inspect the next exact AndroidX/framework blocker. Do not claim 2048 gameplay works until the app reaches a real first frame and interactive play on-device.


### 2026-11-15 — Phase 49: AndroidX lifecycle ArrayList compatibility + startup diagnostics UI

- Reviewed the latest 2048 v1.7.0 session log exported at 15:44. The previous `HashMap` blocker is no longer present. Package/split verification, resource decode, and DEX load still succeed; the first unresolved call is exactly `Ljava/util/ArrayList;-><init>()V`, called by `androidx.lifecycle.LifecycleRegistry.<init>(LifecycleOwner, boolean)` at PC 18. Activity construction fails before any guest instruction executes.
- Added a signature-exact managed `ArrayList.<init>()V` intrinsic with non-null managed receiver validation and initialized empty-list bookkeeping.
- Added a managed indexed ArrayList compatibility subset for lifecycle state stacks: `add(Object)`, `add(index,Object)`, `get(index)`, `set(index,Object)`, `remove(index)`, `remove(Object)`, `contains(Object)`, `size()`, `isEmpty()`, and `clear()`. Indexed storage is held on the list's managed heap instance, with shifting/compaction on insertion/removal and explicit bounds checks. This is a targeted compatibility subset, not a claim of full Java ArrayList implementation or iterator support.
- Added an interpreter regression fixture covering construction, append, indexed lookup, indexed removal/compaction, and size accounting.
- UI extra: runtime result cards now distinguish a completed lifecycle from a blocked startup. A zero-instruction launch explicitly says no guest bytecode executed and directs the user to Session Logs, instead of displaying a green success-style report card for a failed startup.
- Validation: GitHub Actions workflow [run #907](https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/43706088495) passed the Rust runtime test job; the dex-runtime suite reports **227 passed, 0 failed**, including the new ArrayList lifecycle regression. The unsigned IPA build job was skipped because this phase did not request a new IPA build.
- Status: **In Progress — implementation committed; Rust validation passed, iOS compile validation pending.** Do not mark Phase 49 Completed until the iOS compile validation is authorized/run and passes. Device retest is also pending.
- Next: run one consolidated workflow after this implementation batch; correct any failures; then request a fresh 2048 log and continue from the next exact unresolved framework call. Do not claim 2048 has started or reached gameplay until guest instructions, first rendered frame, and interaction are observed.
- All changes remain on `feat/dex-class-method-loader`; `main` untouched. No IPA build, signing, export, or merge was requested in this phase.


### 2026-11-15 — Phase 49 Rust validation checkpoint

- Workflow #907 / run ID `43706088495` completed successfully for the Rust runtime test job.
- The dex-runtime test suite reports **227 passed, 0 failed**, including `array_list_intrinsics_support_lifecycle_state_stack_operations`.
- The iOS/IPA job was skipped by workflow conditions. No new IPA was built, signed, or exported in this phase. This preserves the prior explicit IPA authorization as a one-time build request rather than treating it as blanket permission for every later batch.
- Phase 49 remains **In Progress** until iOS compile validation and subsequent real-device retest. All changes remain on `feat/dex-class-method-loader`; `main` untouched.


### 2026-11-15 — Phase 49 authorized unsigned iOS build checkpoint

- User explicitly authorized a new Stride build in this conversation.
- Triggered the repository's existing consolidated workflow from `feat/dex-class-method-loader`; it runs the complete Rust workspace test suite first, then builds the ARM64 iOS runtime, generates the XCFramework, compiles the iOS app, verifies the app bundle, packages and validates an **unsigned** IPA, and uploads the IPA plus build report as a workflow artifact.
- This authorization covers only the unsigned build. No signing, App Store export, merge to `main`, or device compatibility claim is authorized or implied.
- Phase 49 remains **In Progress** until the consolidated workflow result and real-device retest are reviewed.


### 2026-11-15 — Phase 50: WeakReference lifecycle compatibility + blocker-focused UI

- Reviewed the new 2048 v1.7.0 session log exported at 16:23. Package identity/split validation, resource decode, DEX load, and native pipeline all complete. The previous ArrayList constructor blocker is gone. The first unresolved call is now exactly `Ljava/lang/ref/WeakReference;-><init>(Ljava/lang/Object;)V`, called from `LifecycleRegistry.<init>` at PC 25. The Activity constructor still fails before any guest bytecode executes (0 instructions).
- Added an exact managed `WeakReference.<init>(Object)V` implementation that validates the receiver/referent and stores the referent in the inherited managed `referent` slot. Null referents are preserved.
- Added inherited `Reference.get()Ljava/lang/Object;` and `Reference.clear()V` compatibility operations over that managed slot, matching the owner-reference access pattern needed by AndroidX lifecycle. This does **not** emulate garbage-collector weak reachability or automatic weak-reference clearing.
- Added a regression fixture covering non-null construction, get, clear, null referents, and rejection of a null receiver.
- UI extra: the runtime result card now surfaces the first `[MISSING_API]` event directly as a compact, selectable “First startup blocker” panel, so users can see the exact failing API without manually scanning the full log.
- Validation: pending one consolidated Rust + iOS compile workflow after this implementation batch. No repeated test runs.
- Phase 50 remains **In Progress** until the consolidated workflow passes and a fresh device session log confirms the next startup stage. Do not claim 2048 compatibility or gameplay until guest instructions, a rendered frame, and interaction are observed.
- UI extra: the FIRST STARTUP BLOCKER card now has a one-tap Copy action and a slightly taller selectable diagnostic line, so the exact unresolved API can be shared without manually selecting monospaced text.
- All changes remain on `feat/dex-class-method-loader`; `main` remains untouched. No IPA build/sign/export is included in this phase.


### Phase 50 validation correction

- The first consolidated Rust workflow (#911) exposed a test-only Rust borrow-check error: the new fixture inspected the heap while the interpreter still held its mutable borrow. Runtime implementation itself compiled; the test assertion was moved until after the VM is dropped. The test will now verify the stored referent through the guest-visible `Reference.get()` path before clearing, then inspect the cleared slot after dropping the VM.
- Workflow #911 failed at compile time, so no Phase 50 tests are counted as passing yet. Run one consolidated validation after this correction; do not build an IPA unless separately authorized.


### Phase 50 regression semantics correction

- Workflow #912 compiled successfully and ran the full Rust suite; the new WeakReference fixture was the only failing test (227 passed, 1 failed). The runtime returned `Value::Object(0)` for DEX `return-object` null, which is a valid null representation in this VM (`Value::is_null()` treats it as null).
- Updated the fixture to assert semantic null rather than requiring one internal enum representation. No runtime behavior change was needed for this correction.
- One final consolidated Rust validation is running after this test correction. iOS compile/IPA was skipped; no IPA build is authorized in Phase 50.


### 2026-11-15 — Phase 50 Rust validation checkpoint

- Final consolidated Rust workflow [#913](https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/43709183093) passed.
- The dex-runtime test suite reports **228 passed, 0 failed**, including `weak_reference_constructor_get_and_clear_use_managed_referent`.
- The earlier #911 compile failure and #912 null-representation assertion failure were corrected before this passing run. The final workflow is green.
- At the initial Phase 50 checkpoint the iOS/unsigned IPA job was skipped; the follow-up authorized unsigned build completed successfully in workflow #914. Phase 50 remains **In Progress** pending a fresh device session log.
- All implementation commits remain on `feat/dex-class-method-loader`; `main` is untouched.


### 2026-11-15 — Phase 50 pre-build audit + authorized unsigned IPA checkpoint

- Audited the active branch head `478444a4e311e0e41d2f3a1d1f4599b226f46be7`, the Phase 50 runtime changes, the blocker-focused SwiftUI card, and `.github/workflows/ios-build.yml`.
- Confirmed the previous 2048 session log predates the WeakReference fix and the earlier IPA #910 was built from Phase 49; neither can validate Phase 50 runtime behaviour.
- Confirmed the exact WeakReference constructor and inherited Reference.get/clear intrinsics are present. GC-driven weak reachability/clearing is explicitly unsupported and must not be represented as complete.
- Confirmed the Phase 50 Rust workspace validation passed in workflow #913 (228 tests passed). The iOS/IPA job was skipped in #913, so Swift/Xcode linkage for Phase 50 remains unverified.
- Workflow audit: Rust tests are a required dependency of the iOS job; the job targets generic physical iOS/ARM64, disables code signing, validates the app bundle and IPA ZIP contents, and uploads the unsigned IPA plus build report. No signing or distribution step exists.
- Added a Copy action to the FIRST STARTUP BLOCKER UI card as a small diagnostics usability improvement.
- This checkpoint authorizes one unsigned IPA build only after the Rust test job passes. Do not sign, merge to main, or claim real-device APK execution. Phase 50 remains In Progress until the iOS compile passes and a fresh iPhone session log is reviewed.


### 2026-11-15 — Phase 50 consolidated validation result (workflow #914)

- Rust runtime tests: **Passed** on commit `83c5089e20ede11788ee9a6277f48c3d23b66269`.
- Physical iOS build: **Passed**. ARM64 Rust runtime, device XCFramework, generated app icon, XcodeGen project, package resolution, Release compile for generic physical iPhone, bundle verification, IPA ZIP validation, and artifact upload all succeeded.
- Artifact: `Stride-unsigned-IPA-914` (artifact ID `14160546722`); unsigned IPA SHA-256: `b7380f6765340268265933c74599ae1bd06ef4aedf8f0f9ab03c4b9e33b3e74c`.
- Signing remained disabled. No signed IPA, App Store export, or merge to main occurred.
- Remaining: install this exact IPA on the user's iPhone XS Max (iOS 18.7.10), import the same 2048 v1.7.0 APK, launch once, and inspect the fresh one-shot diagnostic report/session log. Record the next first unresolved API and whether guest instruction count advances. Do not claim 2048 gameplay until a rendered frame and user interaction are verified.
- Phase 50 remains **In Progress** until that real-device retest is reviewed.


### 2026-11-15 — Phase 51: One-Shot Session Diagnostics (implementation batch)

- Began upgrading the existing report foundation into an automatic one-shot diagnostic flow. This is a diagnostics implementation batch; it does not change Android compatibility claims.
- Stride now records APK path, package format, split APK paths and the per-session log path in its active-session marker so an unclean termination can be associated with the exact imported package.
- Previous session/report artifacts are no longer deleted at the next launch before the user can retrieve them. Finished session log paths remain available after the writer is closed.
- On the next app launch, an active marker without a clean end now produces a recovery diagnostic report with the previous APK identity, saved raw session log (when available), start time and an explicit warning that an unclean marker alone does not prove a crash or jetsam.
- Complete reports now read the persisted log even after the session file handle has been closed.
- Runtime UI work: periodic Rust diagnostic snapshots are sampled during guest execution and new runtime lines are appended to the same chronological host session log. Launch-time Rust diagnostics are also captured.
- Session finalization now gathers available guest instruction count, loaded classes, lifecycle state, missing API count, FPS/frame count, guest heap allocation, framebuffer dimensions and latest touch timing; it includes the final Rust runtime snapshot and APK/split fingerprints in one Complete Diagnostic Report.
- Added an explicit **Export Complete Diagnostic Report** action in Session Logs, while retaining raw-log copy/export for troubleshooting.
- Automatic report generation is wired for normal runtime close, launch failure and detail-view dismissal. Unexpected process termination is handled on the next app launch using the persisted marker.
- Limitations remain honest: iOS does not expose a guaranteed jetsam/crash reason or arbitrary register/backtrace data to this app. Such evidence is included only when the runtime/OS actually supplies it. Runtime metrics unavailable at the time of termination remain marked unavailable.
- Validation status: implementation committed; one consolidated repository validation is required next. No IPA build, signing, export or merge to main is included or authorized in this batch.
- Phase 51 status: **In Progress — implementation batch prepared; CI validation and device verification pending.** The next 2048 test should require only one launch/session and one exported Complete Diagnostic Report; no compatibility claim is made until the resulting evidence is reviewed.


### Phase 51 validation workflow adjustment — compile-only iOS job

- Added a dedicated `ios-compile-validation` job to the existing workflow. It runs after the Rust test job, builds the ARM64 runtime/XCFramework and compiles the iOS app for a generic physical iPhone.
- This job stops after verifying the built `.app` bundle. It does **not** package an IPA, sign, export or distribute anything.
- The existing unsigned IPA job remains separately gated behind explicit `workflow_dispatch` authorization or its existing dedicated commit-message condition.
- This lets UI/diagnostics changes receive Swift/Xcode compile validation without creating an IPA.


### Phase 51 implementation review correction

- Final metric capture is ordered before the execution view clears FPS, touch latency and framebuffer state. This ensures the report records the last observed values instead of zeroed UI state.
- Exporting while a guest session is still active now refreshes the report snapshot instead of re-sharing a stale report from an earlier point in that session.


### Phase 51 implementation review correction — empty detail views

- Prevented closing an APK detail page without launching it from generating a misleading “runtime released” report.
- The Complete Diagnostic Report action is available when that APK has a saved report or a session has actually started; a report cannot accidentally attach another APK's last session.


### 2026-11-15 — Phase 51 consolidated validation result

- Final implementation commit: `f40206cd3a1b5b7d62e1ed8201025c84e6376ce8`.
- Consolidated validation: [workflow #918](https://github.com/strideproject77-lgtm/Stride-Official/actions/runs/43719713122) completed successfully.
- Rust workspace tests: **294 passed, 0 failed** across the non-empty test suites (10 + 6 + 36 + 228 + 1 + 13).
- iOS compile-only validation: **Passed** for generic physical iPhone / Release configuration. Rust ARM64 runtime, XCFramework generation, XcodeGen, dependency resolution and app compilation all passed.
- Unsigned IPA job: **Skipped**. No IPA was built, signed or exported.
- One-Shot Diagnostics implementation includes persisted session metadata, automatic clean-session report generation, previous-unclean-session recovery report, periodic Rust diagnostic deltas, final runtime snapshot, guest execution/render/input metrics, package/split fingerprints and a dedicated complete-report export action.
- The report remains explicit about unavailable evidence: an unclean marker is not proof of a crash/jetsam, and iOS does not guarantee access to termination reason, registers or native backtrace.
- Phase 51 status: **Implementation + CI validation Completed; real-device verification Pending.** The next device test should use one 2048 v1.7.0 launch, close the session normally (or let Stride recover an unexpected termination), and export exactly one **Complete Diagnostic Report**. Review whether the report contains the full host/Rust timeline, final snapshot and expected metrics before calling the feature device-verified.
- All changes remain on `feat/dex-class-method-loader`; `main` is untouched. No merge performed.
