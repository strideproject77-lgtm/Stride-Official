# Hill Climb Racing — APKPure XAPK technical handover

**Target:** original Hill Climb Racing (not Hill Climb Racing 2)  
**Android package:** com.fingersoft.hillclimb  
**Developer:** Fingersoft  
**Research date:** 2026-11-10

## Evidence and confidence

This document combines the earlier Stride crash-report metadata with publicly indexed APKPure package information and independent compatibility-port documentation. The user's exact local XAPK has not been uploaded or directly inspected. Therefore version, split count, file hashes, manifest values, ABI contents and native-library hashes below must not be treated as confirmed properties of that local file unless explicitly labelled as public listing data.

### Confirmed from the existing Stride crash report

- Game label: Hill Climb Racing.
- Package: com.fingersoft.hillclimb.
- APK name: com.fingersoft.hillclimb.apk.
- Native library reported: libgame.so.
- Engine/activity evidence: Cocos2dxActivity and Cocos2dxRenderer.
- The report records Cocos2d-x native initialization, GLES2 setup, audio initialization and pthread/thread-construction failures.
- This evidence points to a Cocos2d-x native game path, not a Unity/IL2CPP launch path.

The crash report is diagnostic evidence from a previous run; it is not the original APK/XAPK and does not disclose the exact native library build ID or complete JNI call list.

### Public APKPure listing data (version-specific; not assumed to match user's file)

A publicly indexed APKPure listing for version 1.71.1 (version code 912) identifies:

- XAPK package format.
- Package name com.fingersoft.hillclimb.
- ARM64 variant.
- Download size approximately 155.1 MB.
- Base APK: com.fingersoft.hillclimb.apk.
- ARM64 split: config.arm64_v8a.
- Language split: config.en.
- Density split: config.mdpi.
- APKPure listing signature fingerprint: 9aa52cc5c1ea649b45f295611417b4b6da6324ea.
- Listing-reported XAPK SHA-256: c2850590644d3f8100e38a6cfde0fb3d27cce96ee7b9050583550b0c157f0526.

These are listing values for that specific indexed version/variant, not a fingerprint of the user's local file. APKPure may publish other versions, architectures and split combinations.

A separate public package inventory for version 1.72.2 reports a 156.1 MB APKPure-sourced XAPK with the same package ID. This reinforces that package version and exact split set must be read from the user's actual archive rather than guessed from a web listing.

## Engine and native runtime requirements

The previous crash report and independent HCR compatibility-port research identify the following likely execution requirements:

1. **Cocos2d-x activity/render integration**
   - Correct Activity lifecycle and Cocos2dxActivity/Cocos2dxRenderer-facing JNI surface.
   - Native initialization must be driven by actual guest code, not a simulated Unity state or a hard-coded success result.
   - Frame scheduling and surface lifecycle must be connected to the native renderer.

2. **ARM64 ELF loading**
   - Inspect and load the actual lib/arm64-v8a/libgame.so from the selected ABI split or base APK.
   - Resolve its DT_NEEDED dependencies, dynamic symbols, relocations, constructors and JNI_OnLoad in dependency order.
   - The current Stride native-loader code only produces a checked image/page plan; it does not map ELF segments, relocate symbols or execute guest ARM64 instructions.
   - Do not mark the game Running merely because a library name or entry-point string was found.

3. **JNI and Android compatibility**
   - Build the actual JNI method inventory from the target library and DEX at runtime when the archive is available.
   - Implement only observed signatures and correct return/exception semantics.
   - Cocos2d-x commonly relies on Java-side activity callbacks, asset access, preferences/user defaults, audio and input bridges. Exact HCR signatures remain unverified until extracted from the user's XAPK.

4. **Bionic and threading**
   - Real guest memory allocation and pointer semantics are prerequisites for libc/Bionic functions.
   - pthread create/join/mutex/condition-variable/key/once behavior needs a real guest threading model.
   - Current Stride Bionic placeholders were deliberately changed to fail explicitly rather than fabricate successful pointers or thread handles. Keep this fail-closed behavior until those facilities exist.

5. **Graphics**
   - The crash report indicates GLES2 initialization. HCR's native renderer requires a functioning EGL/GLES2-compatible surface, context lifecycle, required GL entry points, texture/buffer/shader operations and frame presentation.
   - A host-side frame counter or a call to a placeholder swap function is not proof of native rendering.

6. **Audio and assets**
   - Resolve Android AssetManager access against the base APK assets and any relevant expansion data.
   - Implement the actual audio path used by the target (the report records audio initialization; exact API path is not yet confirmed).
   - The current state-only audio session is not evidence of audible playback.

## Current Stride intake pipeline

The active branch already has a Swift importer for APK, XAPK, APKS and APKM:

- It preserves the original distribution container in the package directory.
- It extracts APK components into a persistent components folder.
- For XAPK/APKM, it reads supported manifest metadata keys to identify a base APK when present, then falls back to conventional base names.
- It preserves split APK URLs and exposes the base APK to the inspector.
- Extraction is bounded (512 MiB per component, 1 GiB aggregate) and duplicate flattened filenames are rejected.

Important limitations:

- Import success does not mean split-aware execution is complete.
- The current runtime receives the normalized base APK URL; split-aware class/native/resource lookup is still needed.
- Component extraction now preserves validated archive-relative paths, so equal APK basenames in separate split directories remain distinct; traversal, absolute paths and backslash path tricks are rejected.
- Library restoration recursively discovers nested split APKs, and preflight inspects the base plus every preserved split. This is inventory only: split-aware class/resource/native lookup during execution is still not implemented.
- APKS device-targeted toc.pb selection is not implemented.
- The exact HCR XAPK's manifest and component list remain unknown until its archive is inspected.

## Progress added in the current source-only batch

- Added a base-plus-split inspection path so native ABI folders and DEX inventories from configuration APKs are included in package preflight.
- Updated import and library restoration to retain and rediscover nested split paths without flattening collisions.
- This does not yet select a device-compatible split set, merge Android resources, or make native libraries executable.

## Priority implementation sequence for HCR

### P0 — package correctness and evidence
1. Read XAPK manifest and inventory all entries without extracting the full game payload into memory.
2. Identify the exact base APK and all splits, and validate package name/version consistency across components.
3. Identify the ARM64 split and locate libgame.so; record ELF class, machine, SONAME, DT_NEEDED, build ID, exported/imported symbols and relocations.
4. Decode base APK AXML manifest: launcher Activity, native-library metadata, min/target SDK, permissions, orientation, features and services.
5. Inventory DEX classes/method references and native JNI exports; correlate Java declarations with native registration.
6. Record asset inventory and any OBB/expansion files separately.

### P1 — actual native execution foundation
1. Implement guest address-space allocation and bounded ELF PT_LOAD mapping.
2. Apply RELA/REL relocations and resolve imports from a versioned Android/Bionic symbol table.
3. Load dependencies recursively and run ELF constructors safely.
4. Implement JNI_OnLoad and RegisterNatives with exact signatures and guest references.
5. Implement the observed Cocos2d-x Activity/renderer callbacks and lifecycle.
6. Implement guest pthread primitives and Android asset/preferences bridges.
7. Connect EGL/GLES2 to a real host rendering surface and verify native frame output.
8. Connect actual input and audio callbacks.

### P2 — real game acceptance
- Reach the original Fingersoft loading screen and menu produced by the guest library.
- Confirm native rendering is changing the screen, not a Stride demo surface.
- Confirm menu input, vehicle/gameplay input, pause/resume and orientation behavior.
- Confirm audio where the target expects it.
- Confirm clean exit and diagnostic capture.
- Repeat on the exact recorded XAPK hash and the user's iPhone/iOS version.
- Only then label HCR playable; otherwise publish the precise last successful stage and unresolved symbols/APIs.

## Current honest compatibility status

**Hill Climb Racing: not playable / not verified in Stride.**

The archive format is accepted by the existing importer in principle, and previous crash evidence identifies the Cocos2d-x/libgame.so path. However, Stride does not yet have a real ARM64 guest execution pipeline: its ELF loader is a planning scaffold, native activity launch now fails closed, and Bionic memory/thread APIs reject unsupported calls. The game cannot be considered launched or compatible until the native library is genuinely loaded, initialized, rendered and exercised.

## Research references

- APKPure version 1.71.1 package page: https://apkpure.net/hill-climb-racing/com.fingersoft.hillclimb/download
- Independent HCR PortMaster compatibility-loader notes (describes ARM64 libgame.so, Android bridges, GLES2, assets, audio and limitations): https://github.com/serdarbayram/Hill-Climb-Racing-Portmaster-Version
- Independent HCR native-port notes and historical ARMv7 libgame.so layout: https://github.com/memory-hunter/hill-climb-racing-vita
- Android ABI/native-library packaging documentation: https://developer.android.com/games/optimize/64-bit


---

## Version 1.73.2 research update (2026-11-12)

### Public release identity — independent listing evidence

An APKMirror listing published 2026-10-29 identifies Hill Climb Racing **1.73.2 (versionCode 1016)**:

- Package: `com.fingersoft.hillclimb`
- Minimum Android API varies by bundle variant: API 24, API 29, or API 32.
- Target SDK: API 36.
- Listing includes an ARM64-only variant and ARM64 + ARMv7 variants.
- The ARM64-only 480–640 dpi variant is listed at 111.14 MB (116,543,297 bytes).
- The ARM64 + ARMv7 120–480 dpi bundle is listed at 115.11 MB (120,697,508 bytes), with 32 splits.
- That bundle's APK signing certificate SHA-1 is `9aa52cc5c1ea649b45f295611417b4b6da6324ea`; bundle SHA-256 is `b82460e397fdfcf2e15dd180aed5c11a66b6d6deb7bfe0271a02fd1c606c06e2`.

**Do not assume those APKMirror values identify the user's APKPure archive.** The user's diagnostic log is the strongest evidence for their installed/imported package: it reports 5 APK components (base + 4 splits), 16 DEX files, 7 native libraries and a 571,000-byte resource table. This differs from the public APKMirror 32-split bundle and must be preserved as a distinct package variant until the exact archive manifest is available.

### Engine research — corroborated

- The target is a native-engine game, not Unity/IL2CPP.
- A public compatibility implementation reports successful execution of **HCR 1.67.0** on its own platform and documents the Cocos2d-x JNI render entry point:
  `Java_org_cocos2dx_lib_Cocos2dxRenderer_nativeRender`.
- The same implementation describes JNI entry-point discovery fallback: stock Cocos2d-x symbol, alternate `org.cocos2dx.cpp` spelling, package/class/method-derived Java symbol matching, then `JNI_OnLoad`/native registration.
- It documents APK-backed asset streaming through Cocos2d-x FileUtils and the importance of caching ZIP central-directory reads to avoid repeated linear archive scans on the render thread.
- This is **reference evidence from HCR 1.67.0 and another platform**, not proof that 1.73.2 has identical symbols or can run in Stride. Every symbol and workaround must be revalidated against the target binary before implementation.

### New HCR-specific implementation requirements

1. **Split-aware package view**
   - Treat the base and every split as one logical package, while retaining each component's original archive path.
   - Build one deterministic asset/resource lookup index across all APK components.
   - Resolve duplicate asset paths with Android split/resource precedence rather than arbitrary first-match order.
   - Include all component DEX files in the class path and detect duplicate class descriptors.
   - Select native libraries by device ABI and ensure the chosen `libgame.so` and dependencies all come from a compatible ABI set.
   - Do not use the public 32-split inventory as a substitute for the observed five-component import.

2. **Native symbol and registration discovery**
   - At inspection time, record ELF class, `e_machine`, endianness, SONAME, DT_NEEDED, PT_LOAD, dynamic symbol table, undefined imports, relocation types, GNU build ID, init arrays and exported JNI names.
   - Search for Cocos2d-x renderer hooks including `nativeInit`, `nativeRender`, `nativeOnPause`, `nativeOnResume`, `nativeOnSurfaceCreated`, `nativeOnSurfaceChanged` and input callbacks; treat these as candidate names only, not confirmed exports.
   - Inspect DEX declarations and correlate them with exported JNI symbols and any `RegisterNatives` calls from `JNI_OnLoad`.
   - Preserve exact mangled symbol names and signatures in diagnostics.

3. **Asset I/O**
   - Cocos2d-x may read assets directly from the APK as a ZIP archive. Provide a stable guest-visible APK path and implement correct seek/read/stat/close semantics.
   - Avoid reopening and linearly scanning the ZIP central directory for every small asset read; use a validated central-directory index and bounded caches.
   - Keep asset reads off the UI thread where possible, but preserve ordering and thread-safety expected by the game.
   - Distinguish APK assets, Android resources, expansion/OBB data and writable save data.

4. **Native execution prerequisites**
   - The current Stride native loader is not yet a complete guest ARM64 executor. Do not treat ELF parsing, a dependency inventory, or a resolved symbol name as execution.
   - Required stages remain: guest virtual address mapping; relocation; dependency linking; constructor execution; JNI environment/object/reference semantics; Bionic/libc; real guest threading; EGL/GLES2; audio; input; persistent frame loop.
   - Keep unsupported pthread/memory/JNI calls fail-closed with exact diagnostics until real semantics exist. Do not replace them with success-returning no-op stubs.

### Revised acceptance gates for HCR

- Gate A: exact package identity, component inventory, version consistency and selected ABI verified.
- Gate B: every required ELF dependency and relocation is resolved or explicitly reported; no false-success native loads.
- Gate C: native library constructors and JNI initialization complete without crash.
- Gate D: actual Cocos2d-x surface callbacks and native render loop run; frame output changes over time.
- Gate E: title/menu appears from the guest game, not Stride's host UI; touch input affects the game.
- Gate F: gameplay, pause/resume, orientation, audio, save/load and clean exit are verified.
- Only Gate F permits a “playable” label. Report intermediate gates separately.

### Updated evidence matrix

| Item | Evidence | Confidence |
|---|---|---|
| Package name | User's diagnostic log | High |
| Imported version 1.73.2 | User's filename/session label | Medium; package version code not logged |
| 5 APK components / 4 splits | User's diagnostic log | High |
| 16 DEX files | User's diagnostic log | High |
| 7 native libraries | User's diagnostic log | High |
| 571,000-byte resource table | User's diagnostic log | High |
| Native Cocos2d-x path | Prior crash report + independent HCR port research | High |
| Exact 1.73.2 ELF exports / imports | Not inspected | Unknown |
| Exact five-component split mapping | Not inspected | Unknown |
| Exact APKPure archive hash | Not available | Unknown |
| HCR playable in Stride | Not yet tested | Not established |

### Sources

- APKMirror 1.73.2 ARM64-only variant (release/version/SDK/size): https://www.apkmirror.com/apk/fingersoft/hill-climb-racing/hill-climb-racing-1-73-2-release/hill-climb-racing-1-73-2-2-android-apk-download/
- APKMirror 1.73.2 ARM64 + ARMv7 bundle (32-split example, certificate and bundle hash): https://www.apkmirror.com/apk/fingersoft/hill-climb-racing/hill-climb-racing-1-73-2-release/hill-climb-racing-1-73-2-3-android-apk-download/
- VNX Translation Core (independent HCR 1.67.0 compatibility implementation; JNI and APK asset I/O notes): https://github.com/Viridite/VNX-Translation-Core
- VNX compatibility log (HCR 1.67.0 runtime evidence): https://github.com/Viridite/VNX-Translation-Core/blob/main/compat_log.txt
- OWASP MASTG-TECH-0145 (XAPK layout, base/split/OBB handling): https://mas.owasp.org/MASTG/techniques/android/MASTG-TECH-0145/
- Android ABI guidance: https://developer.android.com/games/optimize/64-bit



---

## 2026-11-12 — ARM64 guest execution foundation added (implementation-only)

The feature branch now contains an initial bounded ARM64 interpreter and an
ELF-backed guest image path. This advances the previous prerequisite from
“no guest ARM64 execution pipeline” to “limited guest instruction execution is
available, but it is not yet sufficient for HCR.”

### Added in this batch

- AArch64 integer/control-flow instruction coverage expanded: MOVZ/MOVK,
  ADD/SUB immediate and shifted-register, logical shifted-register, ADR/ADRP,
  B/BL/B.cond, CBZ/CBNZ, BR/BLR/RET, LDR literal/LDRSW, unsigned-offset LDR/STR,
  and integer LDP/STP addressing forms.
- Guest memory now supports strict mapped-range permissions and executable
  instruction fetch checks; read-only and non-executable pages reject writes or
  instruction fetches respectively.
- Added an overflow-safe per-call instruction budget and bounded guest stack.
- Added `NativeGuestImage` to materialize PT_LOAD images, apply supported
  transactional relocations, map ELF page permissions, and expose guest export
  addresses.
- Added an adapter from the existing `NativeLinker` output. The linker must
  complete dependency loading and `apply_relocations()` without diagnostics
  before its image is handed to the execution adapter.
- Added AAPCS64 x0-x7 integer/pointer argument setup, JNI short/long mangled
  symbol lookup, and bounded guest invocation helpers for exported functions,
  `JNI_OnLoad`, and statically-mangled JNI methods.

### Still not established

- Full ARMv8-A/SIMD/FP support, stack-passed and floating-point ABI arguments,
  Android JavaVM/JNIEnv object/reference semantics, RegisterNatives, JNI callbacks,
  guest threads/TLS/syscalls/signals, and complete Bionic are incomplete.
- Native constructors/init arrays are not yet executed by this path.
- Cocos2d-x initialization, NativeActivity callbacks, EGL/GLES2 real rendering,
  APK-backed Cocos FileUtils semantics, audio/input integration and a persistent
  render loop remain unfinished.
- The HCR-specific ELF dependency graph, exports/imports and actual target archive
  still need to be validated against the user's exact five-component package.
- No HCR loading screen or gameplay success is claimed.

### Validation policy

Regression tests were added but intentionally not run in this batch, per the
user's implementation-only instruction. No iOS workflow was run and no IPA was
built. All changes are on `feat/dex-class-method-loader`; `main` is untouched.


### JNI registration follow-up (same implementation-only batch)

- Added bounded parsing of guest `JNINativeMethod` records (24-byte AArch64 layout), validating readable name/signature strings, executable function pointers, descriptor shape and count limits.
- Added staged/atomic registration storage and dispatch lookup ahead of static JNI-mangled symbol lookup.
- This is a guest metadata/dispatch primitive only. It is not yet connected to the actual `JNIEnv->RegisterNatives` function-table callback; cross-library function pointers and JNI object/reference conversion remain unsupported.
- Tests remain unrun and no IPA was built.


### Follow-up implementation batch — ARM64 coverage and ELF constructors

- Extended the bounded interpreter with logical-immediate operations, conditional-select instructions, narrow byte/halfword memory operations, and unscaled LDUR/STUR forms. Added a regression test for logical-immediate mask decoding.
- Preserved DT_* dynamic metadata in loaded native modules; guest-image creation now discovers and validates DT_INIT and DT_INIT_ARRAY entries against mapped executable pages.
- Added an explicit constructor invocation method. The higher-level loader must call it only after successful dependency relocation and in dependency-first order; this method does not itself load missing dependencies or implement a thread scheduler.
- This is still a limited integer AArch64 interpreter. SIMD/FP, atomics, TLS, syscalls, real JavaVM/JNIEnv callbacks, cross-module guest pointer space, complete Bionic, NativeActivity and EGL/GLES rendering remain blockers.
- Tests were added but not run. No workflow or IPA build was started.

- Added `NativeLinker::dependency_initialization_order` to expose dependency-first ordering for already-loaded native modules. Missing DT_NEEDED entries remain explicit and must be checked before constructor execution; cross-module guest memory and call bridging are still incomplete.


### JNI validation and registration follow-up

- Added strict JNI method descriptor parsing for primitive, object and array parameter/return types. Malformed descriptors and void parameters are rejected.
- Added checked JNI_OnLoad version negotiation for JNI 1.1, 1.2 and 1.4; unsupported version returns are rejected rather than accepted as success.
- Added class-scoped UnregisterNatives state handling and duplicate-entry rejection inside one RegisterNatives table. Registration remains staged atomically.
- These are validation/state-management foundations, not a real guest JNIEnv/JavaVM function table. Guest-callable JNI trampolines, callback dispatch, managed-object/reference conversion and native-to-managed callbacks remain unimplemented.
- Regression tests were added but not run. No workflow or IPA build was started.


### JNI runtime progress (implementation only)
- The JNI environment now has checked local-frame reservation, local-reference creation/deletion, managed-heap root snapshots, and `PopLocalFrame(result)`-style result preservation.
- This reduces reference-lifetime gaps for future native callbacks, but does not yet provide guest-callable `JNIEnv*`/`JavaVM*` tables or native callback trampolines. Global/weak references and automatic root handoff remain outstanding.
- New regression tests are present but have not been run.

### JNI local-reference failure-safety follow-up


### Guest JNI reference-token layer
- Added opaque 64-bit guest reference tokens, deliberately separate from internal `u32` managed object IDs. Local, strong-global and weak-global references now have distinct lifetime classes.
- Added local-frame push/pop with result re-issuance, reference cloning/deletion, guest-handle resolution, same-object checks, strong-root snapshots and weak-reference pruning.
- The table is currently Rust-side infrastructure. It is not yet installed as a guest-callable `JNIEnv*` table and does not provide callable native callback trampolines or automatic GC integration.
- Regression coverage was added but not run; defer all tests to the one final pass.

- Added strong JNI global-reference lifecycle APIs and combined strong-root snapshots so global references can outlive local-frame pops. Weak globals and automatic native-runtime GC root handoff remain future work.
- Made result-preserving frame pop atomic on invalid handles or reservation failure; the active frame remains intact when validation fails.
- Reserve local-reference capacity before creating strings/int/byte arrays, and accept null in NewLocalRef/DeleteLocalRef according to JNI semantics.
- Regression coverage was updated/added; not run yet. Still no guest-callable JNI function table, global/weak references, or automatic GC root handoff.
