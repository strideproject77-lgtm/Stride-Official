# Stride Handoff

Updated: 2026-11-06

## Product goal

Build an iOS app that can run a growing, clearly documented set of Android APKs. The long-term target is broad compatibility; every support claim must name the APK/build, ABI, Android API assumptions, iPhone/iOS version, and real-device result. APK import or static inspection is not execution.

## Working location

- Repository: https://github.com/strideproject77-lgtm/Stride-Official
- Current branch: feat/dex-class-method-loader
- Current branch head: c4037319c3722205e848128dbd3d8140a58efbf7
- Changes are on the feature branch; they have not been merged to main.
- Do not start from a fresh checkout or discard this branch when resuming.

## Work completed on this branch

- Added Rust DEX class/method inventory parsing in runtime-rust/crates/dex-inspector/src/classes.rs.
  - Reads class definitions, encoded field/method lists, class descriptors, method names and prototypes.
  - Checks DEX table bounds, indexes, modified UTF-8 strings and referenced code_item boundaries.
  - This is metadata parsing only; it does not verify or execute app code.
  - Unit tests were added but have not been run.
- Exported the parser from runtime-rust/crates/dex-inspector/src/lib.rs.
- Refreshed Sources/StrideApp/ContentView.swift.
  - Added APK/DEX counts, recent-package rows, package detail sheet and native ABI chips.
  - Added explicit runtime status labels. The UI does not claim APK execution.
  - ABI chips wrap for narrow screens.
- No Husk source files, code, tests, or assets were copied.

## Current app/runtime boundary

The Swift app imports APKs using ZIPFoundation, stores them under Documents/APKs, and inspects ZIP entries. The Rust crates separately contain AXML/DEX/ELF inspection, a bounded integer-only DEX interpreter, and an ELF native-image page planner. The Rust DEX inventory parser is not connected to Swift APK import. There is no working Android execution backend, Bionic/JNI bridge, Android framework, or end-to-end APK launch yet.

## Decisions

1. Continue Stride as an independent Swift/Rust implementation. Do not rewrite the repository from zero and do not copy Husk.
2. Use Android/DEX/ELF/JNI specifications, public documentation, published architecture notes, and black-box behavior to define requirements. Implement independently and maintain original tests.
3. The Husk GitHub repository declares GPL-2.0. High-level documentation and observed behavior can inform requirements. Do not copy or port source, comments, tests, or distinctive implementation structure. Reading GPL code and then reproducing its implementation is not a reliable way to avoid license obligations; obtain a license review before distributing anything materially based on source-level study.
4. Treat “supports most apps” as a long-term goal. Build a measured compatibility matrix and ship support by ABI, API level, framework/service dependencies, and graphics needs. Start with a small real APK cohort and expand from evidence.
5. Prefer an honest partial compatibility report over a fake launch path. Unsupported apps must report why they cannot run.

## CI and unsigned IPA status

The current iOS workflow uses macos-15 for Rust tests, Swift tests, device-target build, and unsigned IPA packaging. Recent runs were cancelled before a runner was assigned; no build step ran. A feature-branch push does not trigger the current push rule, which targets main. A PR targeting main or workflow_dispatch can trigger the macOS workflow.

Recommended workflow maintenance, not implemented yet:
- Run Rust unit tests on ubuntu-latest for each PR/push.
- Reserve macOS for Swift/iOS build and IPA packaging on main or an intentional manual dispatch.
- Keep one final IPA run after code changes are complete; avoid repeated manual dispatches while a run is queued.
- A queued macOS runner is a service-capacity issue when no runner/job steps are assigned; changing build commands does not fix that.

## Known issues and limits

- The new Rust DEX class/method parser has not been compiled or tested in CI.
- The parser is not wired to APKArchiveInspector or the iOS UI.
- Method inventory does not load classes, resolve Android framework calls, or execute methods.
- The integer interpreter supports only a tiny primitive opcode subset; it is not ART.
- Native image planning does not map or execute ELF libraries.
- No real-device end-to-end APK test exists.
- UI changes have not been rendered or built in Xcode; verify them through the macOS workflow when runner capacity is available.
- Current feature work is committed directly to the feature branch in several commits; keep future work on this branch until reviewed.

## Exact next steps

1. Add a fast Ubuntu Rust CI job and move cargo test off the macOS IPA job. Trigger a PR for this feature branch, then fix any compile/test failures in the DEX parser.
2. Design a narrow Rust-to-Swift C ABI for DEX inventory. Build the Rust static library for iOS device and simulator on GitHub's macOS runner.
3. Extract classes*.dex from imported APKs in APKArchiveInspector and call the bridge. Show real class/method counts in the detail screen, with parse errors reported safely.
4. Only after APK parsing is connected and tested, expand execution beyond integer opcodes: DEX verifier, types/objects, method calls and exceptions.
5. Then implement a minimal ARM64 native ABI path (ELF mapping/relocations, Bionic shims and JNI) and a small Android framework surface, starting with real test APKs.

## Resume checklist

- Confirm branch feat/dex-class-method-loader still exists and is based on current main.
- Read this file before making another plan.
- First code task: Ubuntu Rust CI plus parser CI validation; keep the macOS IPA runner for a deliberate final attempt.
