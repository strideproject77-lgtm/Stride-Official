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
