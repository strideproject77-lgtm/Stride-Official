import Foundation
import ZIPFoundation
import StrideDexFFI
import UIKit

/// The execution boundary connecting iOS application requests to the ART managed runtime.
protocol AndroidExecutionBackend {
    func launch(packageAt url: URL, onLog: @escaping (String) -> Void) async throws -> APKExecutionResult
}

enum RuntimeAvailability: Equatable {
    case unavailable(reason: String)
    case available(version: String)
}

struct RuntimeCapabilityReport {
    let availability: RuntimeAvailability
    let supportedABIs: [String]
    let supportedAndroidAPIs: ClosedRange<Int>?
}

/// Owns the Rust runtime beyond the one-shot launch call. The handle is
/// released only when the user closes the app session or the result is discarded.
final class APKRuntimeSession: @unchecked Sendable {
    private let lock = NSLock()
    private var runtime: OpaquePointer?

    init(runtime: OpaquePointer) {
        self.runtime = runtime
    }

    func dispatchTouch(action: Int32, x: Float, y: Float) -> Bool {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return false }
        return stride_runtime_dispatch_touch(runtime, action, x, y) == 0
    }

    func setFocusedGuestText(_ text: String) -> Int32 {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return 1 }
        let bytes = Array(text.utf8)
        return bytes.withUnsafeBufferPointer { buffer in
            stride_runtime_set_focused_text(runtime, buffer.baseAddress, buffer.count)
        }
    }

    func pumpGuestEvents(maxEvents: UInt32 = 8, instructionBudget: Int = 20_000) -> (status: Int32, delivered: UInt32, touchMilliseconds: Float?) {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return (1, 0, nil) }
        var delivered: UInt32 = 0
        let status = stride_runtime_pump_guest_events(runtime, maxEvents, instructionBudget, &delivered)
        guard status == 0 else { return (status, delivered, nil) }
        let measured = stride_runtime_last_touch_processing_ms(runtime)
        return (status, delivered, measured >= 0 ? measured : nil)
    }

    /// Exports the live Rust diagnostic timeline after guest input has been pumped.
    /// Launch-time diagnostics alone cannot show callbacks that happen later.
    func diagnosticsSnapshot() -> String? {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return nil }
        let required = stride_runtime_diagnostics_export(runtime, nil, 0)
        guard required > 1, required <= 1_048_576 else { return nil }
        var buffer = [CChar](repeating: 0, count: Int(required))
        let written = buffer.withUnsafeMutableBufferPointer { storage in
            stride_runtime_diagnostics_export(runtime, storage.baseAddress, storage.count)
        }
        guard written > 1 else { return nil }
        return String(cString: buffer)
    }

    func guestFrameStats() -> (count: UInt64, fps: Float?) {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return (0, nil) }
        let count = stride_runtime_guest_frame_count(runtime)
        let measured = stride_runtime_guest_fps(runtime)
        return (count, measured >= 0 ? measured : nil)
    }

    func copyFramebuffer() -> (data: Data, width: Int, height: Int)? {
        lock.lock();defer{lock.unlock()}
        guard let runtime else{return nil}
        var width:UInt32=0;var height:UInt32=0
        guard stride_runtime_copy_framebuffer(runtime,&width,&height,nil,0)==3,width>0,height>0 else{return nil}
        let count=Int(width)*Int(height);guard count>0,count<=16_777_216 else{return nil}
        let pixels=UnsafeMutablePointer<UInt32>.allocate(capacity:count);defer{pixels.deallocate()}
        guard stride_runtime_copy_framebuffer(runtime,&width,&height,pixels,count)==0 else{return nil}
        return(Data(bytes:pixels,count:count*MemoryLayout<UInt32>.size),Int(width),Int(height))
    }
    func recordFrame(durationMilliseconds: Float) -> Float {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return 0 }
        return stride_runtime_record_frame(runtime, durationMilliseconds)
    }

    func close() {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return }
        self.runtime = nil
        stride_runtime_destroy(runtime)
    }

    deinit {
        close()
    }
}

/// Drives the guest input queue from the iOS display cadence. It is deliberately
/// separate from the runtime session so Close can stop callbacks before freeing
/// the Rust handle.
@MainActor
final class GuestFrameScheduler {
    private var displayLink: CADisplayLink?
    private var previousTimestamp: CFTimeInterval?
    private let onFrame: (Float) -> Void

    init(onFrame: @escaping (Float) -> Void) {
        self.onFrame = onFrame
    }

    func start() {
        guard displayLink == nil else { return }
        let link = CADisplayLink(target: self, selector: #selector(tick(_:)))
        link.add(to: .main, forMode: .common)
        displayLink = link
    }

    func stop() {
        displayLink?.invalidate()
        displayLink = nil
        previousTimestamp = nil
    }

    @objc private func tick(_ link: CADisplayLink) {
        let duration: CFTimeInterval
        if let previousTimestamp {
            duration = max(0.001, link.timestamp - previousTimestamp)
        } else {
            duration = max(0.001, link.duration)
        }
        previousTimestamp = link.timestamp
        onFrame(Float(duration * 1_000))
    }
}

struct APKExecutionResult {
    let statusCode: Int32
    let totalClassesLoaded: UInt32
    let instructionsExecuted: UInt32
    let lifecycleState: String
    let missingApisCount: UInt32
    let averageFps: Float
    let heapAllocatedBytes: UInt32
    let screenOrientation: Int32
    let session: APKRuntimeSession
}

enum RuntimeExecutionError: LocalizedError {
    case unreadableArchive
    case missingManifest
    case missingDex
    case dexLoadFailed(index: Int, status: Int32, detail: String?)
    case nativeLibraryLoadFailed(name: String, status: Int32)
    case runtimeInitializationFailed
    case uncaughtException(String)
    case activityNotFound(String)
    case manifestError(String)

    var errorDescription: String? {
        switch self {
        case .unreadableArchive:
            return "Cannot open APK archive for execution."
        case .missingManifest:
            return "APK is missing AndroidManifest.xml."
        case .missingDex:
            return "APK does not contain a DEX executable."
        case .dexLoadFailed(let index, let status, let detail):
            let reason = detail?.isEmpty == false ? detail! : "Runtime did not provide a detailed diagnostic."
            return "Failed to load DEX file #\(index + 1) (runtime status \(status)): \(reason)"
        case .nativeLibraryLoadFailed(let name, let status):
            return "Failed to load native library \(name) (runtime status \(status))."
        case .runtimeInitializationFailed:
            return "Failed to initialize the ART runtime engine."
        case .uncaughtException(let detail):
            return "Android application crashed with uncaught exception: \(detail)"
        case .activityNotFound(let detail):
            return "Main launcher activity could not be resolved: \(detail)"
        case .manifestError(let detail):
            return "Android manifest parsing error: \(detail)"
        }
    }
}

/// Real ART-compatible managed execution backend for Android applications on iOS.
struct StrideArtExecutionBackend: AndroidExecutionBackend {
    let capabilityReport = RuntimeCapabilityReport(
        availability: .available(version: "ART-Compat-2.0"),
        supportedABIs: ["arm64-v8a"],
        supportedAndroidAPIs: 21...34
    )

    func launch(packageAt url: URL, onLog: @escaping (String) -> Void = { _ in }) async throws -> APKExecutionResult {
        try Task.checkCancellation()
        onLog("[INFO] Launch requested for \(url.lastPathComponent)")
        onLog("[INFO] Opening APK archive")
        guard let archive = Archive(url: url, accessMode: .read) else {
            onLog("[ERROR] Cannot open APK archive for execution")
            throw RuntimeExecutionError.unreadableArchive
        }

        var manifestData = Data()
        var dexDataList: [(source: String, path: String, data: Data)] = []
        var soDataList: [(source: String, name: String, data: Data)] = []
        var assetDataList: [(source: String, path: String, data: Data)] = []
        var assetTotalBytes = 0
        let maxPackageAssetBytes = 256 * 1024 * 1024
        let maxSingleAssetBytes = 64 * 1024 * 1024
        let maxPackageAssetCount = 20_000
        var resourceTableData = Data()
        var splitManifestData: [(source: String, data: Data)] = []

        // XAPK/APKS/APKM imports are normalized to <package>/base.apk and
        // <package>/components/*.apk. Read the base first, then each retained split.
        var packageArchives: [Archive] = [archive]
        var packageArchiveNames = [url.lastPathComponent]
        let componentsURL = url.deletingLastPathComponent().appendingPathComponent("components", isDirectory: true)
        let splitURLs: [URL] = {
            guard let enumerator = FileManager.default.enumerator(
                at: componentsURL,
                includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles]
            ) else { return [] }
            var found: [URL] = []
            for case let candidate as URL in enumerator where candidate.pathExtension.lowercased() == "apk" {
                let isRegularFile = (try? candidate.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) ?? false
                if isRegularFile { found.append(candidate) }
            }
            return found.sorted {
                let left = $0.path.replacingOccurrences(of: componentsURL.path + "/", with: "")
                let right = $1.path.replacingOccurrences(of: componentsURL.path + "/", with: "")
                return left.localizedStandardCompare(right) == .orderedAscending
            }
        }()
        for splitURL in splitURLs {
            try Task.checkCancellation()
            if let splitArchive = Archive(url: splitURL, accessMode: .read) {
                packageArchives.append(splitArchive)
                let relativePath = splitURL.path.replacingOccurrences(of: componentsURL.path + "/", with: "")
                packageArchiveNames.append(relativePath)
            } else {
                onLog("[ERROR] Could not open preserved split APK: \(splitURL.lastPathComponent); refusing partial package execution")
                throw RuntimeExecutionError.unreadableArchive
            }
        }
        onLog("[INFO] Package components discovered: \(packageArchives.count) APK(s), \(max(0, packageArchives.count - 1)) split(s)")

        for (archiveIndex, packageArchive) in packageArchives.enumerated() {
            try Task.checkCancellation()
            for entry in packageArchive {
                try Task.checkCancellation()
                if entry.path == "resources.arsc" {
                    // ResourceCatalog currently accepts one table. Prefer the base table.
                    if resourceTableData.isEmpty {
                        _ = try packageArchive.extract(entry) { chunk in resourceTableData.append(chunk) }
                    }
                } else if entry.path == "AndroidManifest.xml" {
                    if archiveIndex == 0 && manifestData.isEmpty {
                        _ = try packageArchive.extract(entry) { chunk in manifestData.append(chunk) }
                    } else if archiveIndex > 0 {
                        var splitManifest = Data()
                        _ = try packageArchive.extract(entry) { chunk in splitManifest.append(chunk) }
                        let sourceName = packageArchiveNames.indices.contains(archiveIndex)
                            ? packageArchiveNames[archiveIndex]
                            : "package-component-\(archiveIndex)"
                        splitManifestData.append((source: sourceName, data: splitManifest))
                    }
                } else if entry.path == "classes.dex" || entry.path.range(of: #"^classes[0-9]+\.dex$"#, options: .regularExpression) != nil {
                    var dex = Data()
                    _ = try packageArchive.extract(entry) { chunk in dex.append(chunk) }
                    let sourceName = packageArchiveNames.indices.contains(archiveIndex)
                        ? packageArchiveNames[archiveIndex]
                        : "package-component-\(archiveIndex)"
                    dexDataList.append((source: sourceName, path: entry.path, data: dex))
                } else if entry.path.hasPrefix("assets/") && entry.path.count > "assets/".count {
                    let declaredSize = Int(entry.uncompressedSize)
                    guard assetDataList.count < maxPackageAssetCount,
                          declaredSize >= 0,
                          declaredSize <= maxSingleAssetBytes,
                          declaredSize <= maxPackageAssetBytes - assetTotalBytes else {
                        onLog("[WARN] Skipping oversized or excess package asset: \(entry.path)")
                        continue
                    }
                    var asset = Data()
                    _ = try packageArchive.extract(entry) { chunk in asset.append(chunk) }
                    guard asset.count <= maxSingleAssetBytes,
                          asset.count <= maxPackageAssetBytes - assetTotalBytes else {
                        onLog("[WARN] Extracted package asset exceeded memory budget: \(entry.path)")
                        continue
                    }
                    let relativeAssetPath = String(entry.path.dropFirst("assets/".count))
                    let sourceName = packageArchiveNames.indices.contains(archiveIndex)
                        ? packageArchiveNames[archiveIndex]
                        : "package-component-\(archiveIndex)"
                    assetTotalBytes += asset.count
                    assetDataList.append((source: sourceName, path: relativeAssetPath, data: asset))
                } else if entry.path.hasPrefix("lib/arm64-v8a/") && entry.path.hasSuffix(".so") {
                    var so = Data()
                    _ = try packageArchive.extract(entry) { chunk in so.append(chunk) }
                    let name = URL(fileURLWithPath: entry.path).lastPathComponent
                    let sourceName = packageArchiveNames.indices.contains(archiveIndex)
                        ? packageArchiveNames[archiveIndex]
                        : "package-component-\(archiveIndex)"
                    soDataList.append((source: sourceName, name: name, data: so))
                } else if entry.path.hasPrefix("lib/") && entry.path.hasSuffix(".so") {
                    onLog("[WARN] Skipping native library for non-selected ABI: \(entry.path)")
                }
            }
        }

        onLog("[INFO] Archive scan complete: manifest=\(!manifestData.isEmpty), manifest_bytes=\(manifestData.count), DEX=\(dexDataList.count), native libraries=\(soDataList.count), assets=\(assetDataList.count) (\(assetTotalBytes) bytes), APK components=\(packageArchives.count)")
        guard !manifestData.isEmpty else {
            onLog("[ERROR] AndroidManifest.xml is absent or empty")
            throw RuntimeExecutionError.missingManifest
        }

        try Task.checkCancellation()
        var packageName = url.deletingPathExtension().lastPathComponent
        var packageNameBuffer = [CChar](repeating: 0, count: 512)
        let packageNameLength = manifestData.withUnsafeBytes { manifestBuffer -> Int in
            guard let manifestBytes = manifestBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return 0 }
            return packageNameBuffer.withUnsafeMutableBufferPointer { output in
                Int(stride_manifest_package_name(
                    manifestBytes, manifestBuffer.count, output.baseAddress, output.count
                ))
            }
        }
        if packageNameLength > 1, packageNameLength <= packageNameBuffer.count {
            let parsedPackageName = String(cString: packageNameBuffer)
            if !parsedPackageName.isEmpty {
                packageName = parsedPackageName
                onLog("[INFO] Android package identity: \(parsedPackageName)")
            }
        } else {
            onLog("[WARN] Could not resolve package identity from manifest; using archive fallback")
        }
        // A split APK is part of the base package only when its manifest identity
        // matches. Never feed DEX/native code from an unrelated or accidentally
        // mixed archive into this package's runtime.
        for splitManifest in splitManifestData {
            var splitPackageNameBuffer = [CChar](repeating: 0, count: 512)
            let splitPackageNameLength = splitManifest.data.withUnsafeBytes { manifestBuffer -> Int in
                guard let manifestBytes = manifestBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return 0 }
                return splitPackageNameBuffer.withUnsafeMutableBufferPointer { output in
                    Int(stride_manifest_package_name(
                        manifestBytes, manifestBuffer.count, output.baseAddress, output.count
                    ))
                }
            }
            guard splitPackageNameLength > 1, splitPackageNameLength <= splitPackageNameBuffer.count else {
                onLog("[ERROR] Could not verify package identity for split \(splitManifest.source)")
                throw RuntimeExecutionError.manifestError("Could not verify package identity for split \(splitManifest.source)")
            }
            let splitPackageName = String(cString: splitPackageNameBuffer)
            guard splitPackageName == packageName else {
                onLog("[ERROR] Split package mismatch: \(splitManifest.source) belongs to \(splitPackageName), expected \(packageName)")
                throw RuntimeExecutionError.manifestError("Split \(splitManifest.source) belongs to \(splitPackageName), expected \(packageName)")
            }
            onLog("[INFO] Split package identity verified: \(splitManifest.source) -> \(splitPackageName)")
        }
        onLog("[INFO] Resource table: \(resourceTableData.count) bytes")
        onLog("[INFO] Initializing ART-compatible runtime")
        guard let runtime = packageName.withCString({ stride_runtime_create($0) }) else {
            onLog("[FATAL] Rust runtime creation returned null")
            throw RuntimeExecutionError.runtimeInitializationFailed
        }
        var runtimeOwnershipTransferred = false
        defer {
            if !runtimeOwnershipTransferred {
                stride_runtime_destroy(runtime)
            }
        }
        try Task.checkCancellation()
        var registeredAssets = 0
        for (index, asset) in assetDataList.enumerated() {
            try Task.checkCancellation()
            let status = asset.path.withCString { pathPointer in
                asset.data.withUnsafeBytes { raw in
                    stride_runtime_add_asset(
                        runtime, pathPointer,
                        raw.baseAddress?.assumingMemoryBound(to: UInt8.self),
                        raw.count
                    )
                }
            }
            if status == 0 {
                registeredAssets += 1
            } else {
                onLog("[WARN] Could not register package asset \(asset.source)!assets/\(asset.path) (status \(status))")
            }
            if (index + 1).isMultiple(of: 256) {
                onLog("[INFO] Package asset registration progress: \(index + 1)/\(assetDataList.count)")
            }
        }
        onLog("[INFO] Registered \(registeredAssets)/\(assetDataList.count) package assets for Android AssetManager (\(assetTotalBytes) bytes scanned)")
        if !resourceTableData.isEmpty {
            let resourceStatus = resourceTableData.withUnsafeBytes { raw in
                stride_runtime_load_resource_table(runtime, raw.bindMemory(to: UInt8.self).baseAddress, resourceTableData.count)
            }
            if resourceStatus == 0 { onLog("[INFO] Android resource table decoded") }
            else { onLog("[WARN] Resource table decode unavailable (status \(resourceStatus)); literal UI text remains available") }
        }
        if !resourceTableData.isEmpty {
            // Android resource tables in games can contain hundreds or thousands of
            // file references. Eagerly decoding every image before DEX/native startup
            // can stall launch and allocate several large temporary pixel buffers.
            // Keep this compatibility path bounded; Cocos game assets are loaded via
            // the package asset/native path rather than this UI-resource cache.
            let fileCount = stride_runtime_resource_file_count(runtime)
            let maxResourceFilesToScan: UInt32 = 1_024
            let maxDecodedResourceImages = 128
            let maxTotalResourceImagePixels = 8_388_608
            let scanCount = min(fileCount, maxResourceFilesToScan)
            var decodedImages = 0
            var decodedImagePixels = 0
            var scannedResourceFiles = 0
            onLog("[INFO] Resource image scan: \(fileCount) entries; bounded to \(scanCount) entries, \(maxDecodedResourceImages) images and \(maxTotalResourceImagePixels) pixels")
            for index in 0..<scanCount {
                try Task.checkCancellation()
                scannedResourceFiles += 1
                if scannedResourceFiles.isMultiple(of: 128) {
                    onLog("[INFO] Resource image scan progress: \(scannedResourceFiles)/\(scanCount)")
                }
                var resourceID: UInt32 = 0
                var pathBuffer = [UInt8](repeating: 0, count: 1024)
                let pathLength = pathBuffer.withUnsafeMutableBufferPointer { buffer in
                    stride_runtime_resource_file_entry(runtime, index, &resourceID, buffer.baseAddress, buffer.count)
                }
                guard pathLength > 1, pathLength <= pathBuffer.count,
                      let path = String(bytes: pathBuffer.prefix(pathLength - 1), encoding: .utf8),
                      let located = packageArchives.lazy.compactMap({ candidate -> (Archive, Entry)? in
                          guard let match = candidate.first(where: { $0.path == path }) else { return nil }
                          return (candidate, match)
                      }).first else { continue }
                let (resourceArchive, entry) = located
                var encoded = Data()
                _ = try resourceArchive.extract(entry) { encoded.append($0) }
                guard let image = UIImage(data: encoded), let cgImage = image.cgImage else { continue }
                let width = cgImage.width, height = cgImage.height
                let pixelCountResult = width.multipliedReportingOverflow(by: height)
                guard width > 0, height > 0, !pixelCountResult.overflow,
                      pixelCountResult.partialValue <= 4_194_304 else { continue }
                let pixelCount = pixelCountResult.partialValue
                guard decodedImages < maxDecodedResourceImages,
                      pixelCount <= maxTotalResourceImagePixels - decodedImagePixels else { continue }
                var rgba = [UInt8](repeating: 0, count: pixelCount * 4)
                let rendered = rgba.withUnsafeMutableBytes { raw -> Bool in
                    guard let context = CGContext(data: raw.baseAddress, width: width, height: height,
                        bitsPerComponent: 8, bytesPerRow: width * 4,
                        space: CGColorSpaceCreateDeviceRGB(),
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
                    context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
                    return true
                }
                guard rendered else { continue }
                var argb = [UInt32](repeating: 0, count: pixelCount)
                for pixel in 0..<pixelCount {
                    let p = pixel * 4
                    argb[pixel] = (UInt32(rgba[p + 3]) << 24) | (UInt32(rgba[p]) << 16) | (UInt32(rgba[p + 1]) << 8) | UInt32(rgba[p + 2])
                }
                let status = argb.withUnsafeBufferPointer { buffer in
                    stride_runtime_set_resource_image(runtime, resourceID, UInt32(width), UInt32(height), buffer.baseAddress, buffer.count)
                }
                if status == 0 {
                    decodedImages += 1
                    decodedImagePixels += pixelCount
                }
            }
            let skippedEntries = fileCount - scanCount
            onLog("[INFO] Decoded \(decodedImages) Android image resources (\(decodedImagePixels) pixels); scanned \(scannedResourceFiles)/\(fileCount), deferred \(skippedEntries) entries")
        }
        // A manifest without executable DEX code cannot enter the managed runtime.
        guard !dexDataList.isEmpty else {
            onLog("[ERROR] No classes*.dex entries found in APK")
            throw RuntimeExecutionError.missingDex
        }

        // Load every DEX and stop immediately if the runtime rejects one.
        for (index, dexEntry) in dexDataList.enumerated() {
            try Task.checkCancellation()
            let dex = dexEntry.data
            let dexLabel = "\(dexEntry.source)!\(dexEntry.path)"
            onLog("[INFO] Loading DEX file \(index + 1)/\(dexDataList.count): \(dexLabel) (\(dex.count) bytes)")
            let dexStatus = dex.withUnsafeBytes { rawBuffer -> Int32 in
                guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return 1
                }
                return stride_runtime_load_dex(runtime, ptr, rawBuffer.count)
            }
            try Task.checkCancellation()
            guard dexStatus == 0 else {
                onLog("[ERROR] DEX \(index + 1) \(dexLabel) load failed with status \(dexStatus)")

                // Rust records the underlying parser/loader error in the runtime's
                // bounded diagnostic timeline. Export it before destroying runtime.
                let required = stride_runtime_diagnostics_export(runtime, nil, 0)
                var report = ""
                if required > 1 {
                    var buffer = [CChar](repeating: 0, count: Int(required))
                    let written = buffer.withUnsafeMutableBufferPointer { storage in
                        stride_runtime_diagnostics_export(runtime, storage.baseAddress, storage.count)
                    }
                    if written > 1 {
                        report = String(cString: buffer)
                        for line in report.components(separatedBy: .newlines) where !line.isEmpty {
                            onLog("[RUNTIME] \(line)")
                        }
                    }
                }
                let detail = report.components(separatedBy: .newlines)
                    .last(where: { $0.contains("DEX load failed") || $0.contains("DEX class inspection failed") })
                    ?? "No detailed DEX diagnostic was recorded."
                throw RuntimeExecutionError.dexLoadFailed(index: index, status: dexStatus, detail: "\(dexLabel): \(detail)")
            }
            onLog("[INFO] DEX file \(index + 1) loaded successfully: \(dexLabel) (\(dex.count) bytes)")
        }

        // Load native shared libraries (.so) into the Bionic/ELF linker
        for nativeEntry in soDataList {
            try Task.checkCancellation()
            let soName = nativeEntry.name
            let soData = nativeEntry.data
            let nativeLabel = "\(nativeEntry.source)!lib/arm64-v8a/\(soName)"
            onLog("[INFO] Loading native library \(nativeLabel) (\(soData.count) bytes)")
            let nativeStatus = soData.withUnsafeBytes { rawBuffer -> Int32 in
                guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return 1
                }
                return soName.withCString { namePtr in
                    stride_runtime_load_so(runtime, namePtr, ptr, rawBuffer.count)
                }
            }
            try Task.checkCancellation()
            guard nativeStatus == 0 else {
                // Best-effort launch: a failed optional/unsupported .so must not
                // prevent trying the remaining libraries and managed Activity.
                // Apps that require this library may still fail later at the call site.
                onLog("[WARN] Native library \(nativeLabel) could not be materialized (status \(nativeStatus)); continuing best-effort execution")
                continue
            }
            onLog("[INFO] Native ELF image \(nativeLabel) materialized; guest entry points and JNI_OnLoad are not yet executed")
        }
        let unresolvedNativeModules = stride_runtime_finish_native_loading(runtime)
        if unresolvedNativeModules == UInt32.max {
            onLog("[ERROR] Native dependency finalization failed")
        } else if unresolvedNativeModules > 0 {
            onLog("[WARN] Native dependency scan found \(unresolvedNativeModules) module(s) with unloaded DT_NEEDED libraries")
        } else {
            onLog("[INFO] All declared native-library dependencies are present in the loaded package set")
        }

        var screenOrientation: Int32 = -1
        let orientationStatus = manifestData.withUnsafeBytes { rawBuffer -> Int32 in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else { return 1 }
            return stride_manifest_screen_orientation(ptr, rawBuffer.count, &screenOrientation)
        }
        if orientationStatus == 0 {
            onLog("[INFO] APK launcher screenOrientation enum=\(screenOrientation)")
        } else {
            onLog("[WARN] Could not read APK screenOrientation (status=\(orientationStatus)); using automatic iPhone orientation")
            screenOrientation = -1
        }

        onLog("[INFO] Dispatching launcher Activity; instruction budget=200000")
        var report = StrideLaunchReport()
        let status = manifestData.withUnsafeBytes { rawBuffer -> Int32 in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return 1
            }
            return stride_runtime_launch_apk(runtime, ptr, rawBuffer.count, 200_000, &report)
        }

        // Export the complete Rust-side timeline after every launch attempt,
        // not only when DEX loading fails. This includes missing APIs, lifecycle
        // diagnostics, interpreter faults and bounded-history overflow.
        let diagnosticBytesRequired = stride_runtime_diagnostics_export(runtime, nil, 0)
        if diagnosticBytesRequired > 1 {
            var diagnosticBuffer = [CChar](repeating: 0, count: Int(diagnosticBytesRequired))
            let diagnosticBytesWritten = diagnosticBuffer.withUnsafeMutableBufferPointer { storage in
                stride_runtime_diagnostics_export(runtime, storage.baseAddress, storage.count)
            }
            if diagnosticBytesWritten > 1 {
                let diagnosticReport = String(cString: diagnosticBuffer)
                for line in diagnosticReport.components(separatedBy: .newlines) where !line.isEmpty {
                    onLog("[RUNTIME] \(line)")
                }
            }
        }

        guard status == 0 else {
            onLog("[ERROR] Runtime launch bridge failed with status \(status)")
            throw RuntimeExecutionError.runtimeInitializationFailed
        }

        let lifecycleDescription: String
        switch report.lifecycle_state {
        case 0: lifecycleDescription = "NotStarted"
        case 1: lifecycleDescription = "Created"
        case 2: lifecycleDescription = "Started"
        case 3: lifecycleDescription = "Resumed"
        case 4: lifecycleDescription = "Paused"
        case 5: lifecycleDescription = "Stopped"
        default: lifecycleDescription = "Destroyed"
        }

        let runtimeStatusDescription: String
        switch report.status_code {
        case 0: runtimeStatusDescription = "GuestRuntimeActive"
        case 1: runtimeStatusDescription = "FinishedNormally"
        case 2: runtimeStatusDescription = "UncaughtException"
        case 3: runtimeStatusDescription = "ActivityNotFound"
        case 4: runtimeStatusDescription = "ManifestError"
        case 5: runtimeStatusDescription = "LifecycleReady (guest loop inactive)"
        default: runtimeStatusDescription = "UnknownRuntimeStatus"
        }
        onLog("[INFO] Runtime returned status=\(runtimeStatusDescription)[\(report.status_code)], lifecycle=\(lifecycleDescription), classes=\(report.total_classes_loaded), instructions=\(report.instructions_executed), missing APIs=\(report.missing_apis_count)")
        if report.status_code == 2 {
            onLog("[CRASH] Runtime reported an uncaught Activity lifecycle exception")
            throw RuntimeExecutionError.uncaughtException("Runtime reported uncaught exception during Activity lifecycle")
        } else if report.status_code == 3 {
            onLog("[ERROR] Launcher Activity was not found")
            throw RuntimeExecutionError.activityNotFound("Launcher Activity not found in DEX")
        } else if report.status_code == 4 {
            onLog("[ERROR] Binary Android manifest parsing failed")
            throw RuntimeExecutionError.manifestError("Failed to parse AndroidManifest binary XML")
        }

        // Transfer ownership to the UI so the Rust heap, class loader, graphics
        // surface and input queue are not destroyed when this launch function returns.
        // This retains runtime state; it does not yet imply a guest instruction scheduler.
        let session = APKRuntimeSession(runtime: runtime)
        runtimeOwnershipTransferred = true
        onLog("[INFO] Runtime handle retained after lifecycle dispatch")
        onLog("[INFO] Runtime session retained; host display-link will pump queued guest input and refresh the guest surface")
        return APKExecutionResult(
            statusCode: report.status_code,
            totalClassesLoaded: report.total_classes_loaded,
            instructionsExecuted: report.instructions_executed,
            lifecycleState: lifecycleDescription,
            missingApisCount: report.missing_apis_count,
            averageFps: report.average_fps,
            heapAllocatedBytes: report.heap_allocated_bytes,
            screenOrientation: screenOrientation,
            session: session
        )
    }
}
