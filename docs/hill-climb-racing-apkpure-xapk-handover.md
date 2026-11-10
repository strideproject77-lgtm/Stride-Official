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
