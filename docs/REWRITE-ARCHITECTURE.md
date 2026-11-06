# Stride native runtime rewrite

## Direction
Stride is an independent iOS application-compatibility runtime. The target is to execute supported Android application code in the host process through native translation and compatibility layers. A booted Android guest is not the target architecture. APK inspection and a UI launch are never evidence of execution.

## Research boundary
Husk was studied as an architectural reference. Its current primary path boots an ARM64 Android guest with QEMU/TCG; its separate no-guest translation layer is experimental. Useful engineering constraints include iOS executable-memory policy, split writable/executable mappings, 16 KiB page planning, ELF segment layout, Android ABI differences, ART integration, and UIKit/Metal adaptation.

Stride is implemented independently. Do not copy or port source files from Husk, QEMU, ATL, AOSP, or other projects without a separate compatibility and licensing review. Research notes may describe interfaces, constraints, and documented behavior; implementation must be original and appropriately licensed.

## Runtime layers
1. Package intake — APK/ZIP validation, binary manifest semantics, DEX and native-library inventory, signing and split-package metadata.
2. DEX foundation — complete instruction decoding, verifier, type model, register frames, heap/object model, exceptions, class linking and resolution.
3. Managed execution — interpreter first; profile-guided compilation and a platform-legal JIT backend only after execution semantics and memory policy are tested.
4. Android native ABI — ELF32/ELF64 inspection, 16 KiB-aware image layout, relocation and symbol resolution, Bionic-facing ABI shims, thread-local state, signals, file descriptors and system-call translation.
5. JNI — VM/thread attachment, reference tables, local/global references, method and field IDs, marshaling, exception propagation and native library lifecycle.
6. Android framework — process/application model, Context, PackageManager, Activity lifecycle, resources, views, input, storage, networking and intents.
7. Device backends — UIKit presentation, Metal graphics, OpenGL ES translation, CoreAudio, touch, orientation, memory-pressure handling and diagnostics.

## Evidence gates
- A parser test proves parsing, not loading.
- A bytecode unit test proves an opcode subset, not ART compatibility.
- A mapping plan proves address/permission calculations, not that Darwin accepted the mapping.
- A JIT allocator self-test must execute generated code on supported hardware.
- A framework test must launch a real APK entry point and display its own UI.
- A game compatibility claim requires a named APK build, device/iOS version, launch result, rendering/input/audio observations, and sustained-run result.

Every capability exposed in the app must map to an implementation and evidence gate. Unsupported features remain explicit; never route an imported APK to a built-in sample screen.

## Current milestone
**M1 — Native ELF mapping plan.** The Rust native-loader crate converts inspected PT_LOAD segments into checked 16 KiB page layouts, merges page permissions and rejects writable-executable pages. This is planning only; actual Darwin memory mapping, relocation writes, Bionic, JNI, and native execution remain unimplemented.
