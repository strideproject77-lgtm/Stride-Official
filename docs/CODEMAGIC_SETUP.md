# Stride CodeMagic builder

Stride has an independent CodeMagic macOS builder. It checks out this repository's `main` branch and builds the same Stride source tree as GitHub Actions.

## Isolation from INFINITY

- Stride logs are published only to this repository's `stride-codemagic-logs` branch.
- INFINITY's `codemagic-logs` branch and `INFINITY_LOGS_GITHUB_TOKEN` are not used.
- Configure a separate GitHub token named `STRIDE_LOGS_GITHUB_TOKEN`, scoped only to `strideproject77-lgtm/Stride-Official`, with Contents: Read and write.
- The log branch is an orphan-style snapshot containing only `latest/` build logs and metadata. It does not contain source files.
- If the token is absent or log publishing fails, the build still leaves logs in CodeMagic artifacts and reports a warning.

## One-time CodeMagic setup

1. Sign in to CodeMagic and connect the GitHub account that can access `strideproject77-lgtm/Stride-Official`.
2. Add the `Stride-Official` repository as a CodeMagic application.
3. Select `codemagic.yaml` as the workflow configuration file.
4. Add the environment variable `STRIDE_LOGS_GITHUB_TOKEN` in CodeMagic's secure environment-variable settings. Use a fine-grained token restricted to this repository with Contents: Read and write. Do not reuse INFINITY's token.
5. Enable the `stride-ios` workflow. Pushes to `main` trigger it automatically.
6. The first successful log publication creates the isolated `stride-codemagic-logs` branch.

## Build outputs

- Unsigned iPhone IPA: `build/ios/Stride-unsigned.ipa`
- Full build transcript and step logs: CodeMagic artifacts
- Latest GitHub log snapshot: `stride-codemagic-logs:latest/`

This workflow builds an unsigned IPA. It does not sign, install, or publish to the App Store.
