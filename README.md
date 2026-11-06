# Stride

Stride is an independent native iOS project exploring a practical Android application compatibility runtime.

## Project direction

- Native iOS application written primarily in Swift.
- Native translation/runtime components, with Rust used where it offers a concrete systems-level advantage.
- APK inspection and compatibility analysis are separate from execution.
- A genuine execution pipeline; no Android virtual machine, simulated Android desktop, or fake app-launch success.

## Current status

The repository has been initialized. Android application execution is not implemented yet. The current foundation provides a native iOS shell, APK archive inspection, an explicit runtime boundary, and safe ELF identification primitives. Execution support will only be claimed after an actual APK runs through the runtime on a supported physical device.

## Architecture

- **StrideApp** — native iOS user interface and app lifecycle.
- **APK intake** — package import, archive inspection, and validation.
- **Runtime boundary** — execution contracts and capability reporting.
- **RuntimeRust** — isolated low-level components where Rust offers a concrete advantage.

## Development

The Xcode project is generated from `project.yml` using XcodeGen. The GitHub Actions workflow generates the project, builds the iOS Simulator target, and runs Rust unit tests.

## Compatibility policy

A feature is not implemented merely because its UI exists. Each runtime capability needs tests and device-level evidence. Unsupported Android formats, ABIs, bytecode, native libraries, and framework APIs must be reported honestly.
