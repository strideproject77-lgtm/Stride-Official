# Stride

Stride is an independent native iOS project exploring a practical Android application compatibility runtime.

## Project direction

- Native iOS application written primarily in Swift.
- Native translation/runtime components, with Rust used where it offers a concrete systems-level advantage.
- APK inspection and compatibility analysis as explicit, separate stages from execution.
- A real execution pipeline; no Android virtual machine, simulated Android desktop, or fake app-launch success.

## Current status

This repository has just been initialized. No Android application execution capability is claimed yet. The initial work establishes the native app shell, runtime boundaries, build configuration, and testable foundations. APK execution will only be reported as supported after an actual APK runs through the runtime on a supported device.

## Architecture

- **StrideApp** — native iOS user interface and app lifecycle.
- **APKKit** — package intake, archive inspection, metadata models, and validation.
- **RuntimeCore** — execution contracts, capability reporting, diagnostics, and lifecycle coordination.
- **NativeBridge** — narrow Swift/Rust interoperability boundary for future native components.
- **RuntimeRust** — Rust workspace for isolated low-level components when justified.

## Development

The Xcode project is generated from `project.yml` using XcodeGen. Open `Stride.xcodeproj` after generating it, or use the repository GitHub Actions workflow for automated validation.

## Compatibility policy

A feature is not considered implemented merely because its UI exists. Each runtime capability must have tests and device-level evidence. Unsupported Android formats, ABIs, bytecode, native libraries, and framework APIs must be reported honestly.
