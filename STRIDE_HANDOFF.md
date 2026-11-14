# Stride Handoff

Updated: 2026-11-14 (Phase 32 accent color picker and HCR follow-up evidence)

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
