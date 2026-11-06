# Stride

Stride is an independent native iOS project exploring a practical Android application compatibility runtime.

## Project direction

- Native iOS application written primarily in Swift.
- Native translation/runtime components, with Rust used where it offers a concrete systems-level advantage.
- APK inspection and compatibility analysis are separate from execution.
- A genuine execution pipeline; no Android virtual machine, simulated Android desktop, or fake app-launch success.

## Current status

The native iOS shell, APK archive intake, Android binary XML manifest metadata extraction, DEX structural decoding, and ELF dynamic metadata inspection are in place. A first bounded Dalvik integer interpreter now executes a documented subset of primitive DEX instructions in isolated tests. It is not ART, does not load classes or APK methods, and does not yet support objects, strings, exceptions, JNI, Android framework calls, or native libraries. No Android APK execution is currently claimed.

## Architecture

- **StrideApp** — native iOS user interface and app lifecycle.
- **APK intake** — package import, archive inspection, and validation.
- **Runtime boundary** — explicit execution contracts and capability reporting.
- **RuntimeRust** — DEX execution primitives and low-level binary components where Rust offers a concrete advantage.

## Development

The Xcode project is generated from `project.yml` using XcodeGen. The GitHub Actions workflow generates the project, builds the iOS Simulator target, and runs Rust unit tests.

## Compatibility policy

A feature is not implemented merely because its UI exists. Each runtime capability needs tests and device-level evidence. Unsupported Android formats, ABIs, bytecode, native libraries, and framework APIs must be reported honestly.
