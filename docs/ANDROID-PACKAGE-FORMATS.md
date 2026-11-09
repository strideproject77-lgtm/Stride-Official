# Android package format intake

Stride's import pipeline accepts APK, XAPK, APKS and APKM file extensions.

- APK: copied into a stable package directory as base.apk.
- XAPK: reads manifest.json split_apks metadata when available, identifies the base APK, and preserves all APK components.
- APKM: reads manifest.json or info.json base/split metadata when available, then extracts the APK components.
- APKS: selects base-master.apk or base.apk and preserves the remaining APK components. toc.pb targeting and device-specific split selection are not implemented yet.

Extraction limits: 512 MiB per APK component and 1 GiB total APK payload. ZIP paths are reduced to safe filenames, duplicate names are rejected, and failed imports clean up their incomplete package directory.

IMPORTANT: successful format import is not equivalent to full split-runtime execution. The current launcher receives the normalized base APK URL. Split-aware class/resource resolution, APKS toc.pb parsing, device-compatible split selection, and end-to-end execution remain pending. Do not claim universal XAPK/APKS/APKM compatibility until these are implemented and tested.

## 2048 target
The supplied com.tpcstld.twozerogame 1.7.0 XAPK contains a base APK and 18 configuration APKs. This importer preserves the original XAPK and all component APKs while exposing the base APK to the existing inspector/runtime pipeline. Next runtime work: decode the actual manifest, resolve the real launcher, execute its DEX, and satisfy Canvas/View/touch APIs in the 2048 handover.
