# Stride Handoff

Updated: 2026-11-07

## Product goal

Build an iOS app that can run a growing, clearly documented set of Android APKs. The long-term target is broad compatibility; every support claim must name the APK/build, ABI, Android API assumptions, iPhone/iOS version, and real-device result. APK import or static inspection is not execution.

## Working location

- Repository: https://github.com/strideproject77-lgtm/Stride-Official
- Continue on branch: feat/dex-class-method-loader
- Changes are on the feature branch; they have not been merged to main.
- Fetch the branch head when resuming; this handoff itself is committed after the code/UI/workflow commits.
- Do not start from a fresh checkout or discard this branch.

## Work completed on this branch

- Added Rust DEX class/method inventory parsing in runtime-rust/crates/dex-inspector/src/classes.rs.
  - Reads class definitions, encoded field/method lists, class descriptors, method names and prototypes.
  - Checks DEX table bounds, indexes, modified UTF-8 strings and referenced code_item boundaries.
  - Metadata parsing only; it does not verify or execute app code.
  - Unit tests are included and passed in PR workflow run #19.
- Exported the parser from runtime-rust/crates/dex-inspector/src/lib.rs.
- Refreshed Sources/StrideApp/ContentView.swift.
  - Added APK/DEX counts, recent-package rows, package detail sheet and native ABI chips.
  - Added explicit runtime status labels. The UI does not claim APK execution.
  - ABI chips wrap for narrow screens.
- Updated .github/workflows/ios-build.yml.
  - Rust tests run on ubuntu-latest.
  - The unsigned iOS build runs on macos-15 after Rust tests, for main and the Codex feature branch; PR runs skip the macOS job.
  - The Codex branch now triggers CI on push.
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

The CI workflow has been split by runner:
- PRs run Rust tests on Ubuntu; the macOS job is skipped.
- Pushes to main and feat/dex-class-method-loader run Rust tests first, then the macOS physical-device build and unsigned IPA packaging.
- The macOS workflow intentionally does not run simulator-based Swift tests because the hosted Mac runner reported "Unable to connect to simulator" during xcodebuild -downloadPlatform iOS (run #24). The app's Swift sources are still compiled by the physical-device build; simulator test execution remains unverified and must be run on a runner with a working Simulator service.
- Run #23 on the feature branch passed Rust tests but failed Swift tests because the requested iPhone 17 Pro simulator did not exist.
- Run #24 fixed the hard-coded simulator selection but failed while attempting to download the iOS Simulator runtime: "Unable to connect to simulator."
- Workflow commit 913a9f3 removes the simulator download/device-selection/test steps so the IPA build is no longer blocked by the unavailable Simulator service. The next run must verify the physical-device build and artifact upload.

## Known issues and limits

- The DEX class/method parser compiled and passed its Rust tests in CI run #19.
- The parser is not wired to APKArchiveInspector or the iOS UI.
- Method inventory does not load classes, resolve Android framework calls, or execute methods.
- The integer interpreter supports only a tiny primitive opcode subset; it is not ART.
- Native image planning does not map or execute ELF libraries.
- No real-device end-to-end APK test exists.
- UI changes have not been rendered or built in Xcode; verify them through the macOS workflow when runner capacity is available.
- CI run #18 also exposed an old native-loader test failure: address overflow returned InvalidAlignment first. The validation order was fixed in the next commit; run #19 passed.

## Exact next steps

1. PR #1 is open as a draft. Recheck CI on the latest feature-branch commit; do not merge until the full checks and review pass.
2. Verify the physical-device build, IPA packaging, and artifact upload on the latest workflow run; inspect the IPA before making UI claims.
3. Implement a narrow Rust-to-Swift C ABI for DEX inventory. Build the Rust static library for iOS device and simulator on GitHub's macOS runner.
4. Extract classes*.dex from imported APKs in APKArchiveInspector and call the bridge. Show real class/method counts in the detail screen, with parse errors reported safely.
5. Only after APK parsing is connected and tested, expand execution beyond integer opcodes: DEX verifier, types/objects, method calls and exceptions.
6. Then implement a minimal ARM64 native ABI path (ELF mapping/relocations, Bionic shims and JNI) and a small Android framework surface, starting with real test APKs.

## Resume checklist

- Confirm branch feat/dex-class-method-loader exists and compare it with current main.
- Read this file before making another plan.
- First code action: build the Rust-to-Swift DEX inventory bridge; keep macOS for a deliberate IPA build attempt.
