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

    func pumpGuestEvents(maxEvents: UInt32 = 8, instructionBudget: Int = 20_000) -> (status: Int32, delivered: UInt32) {
        lock.lock()
        defer { lock.unlock() }
        guard let runtime else { return (1, 0) }
        var delivered: UInt32 = 0
        let status = stride_runtime_pump_guest_events(runtime, maxEvents, instructionBudget, &delivered)
        return (status, delivered)
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
        supportedABIs: ["arm64-v8a", "armeabi-v7a"],
        supportedAndroidAPIs: 21...34
    )

    func launch(packageAt url: URL, onLog: @escaping (String) -> Void = { _ in }) async throws -> APKExecutionResult {
        onLog("[INFO] Launch requested for \(url.lastPathComponent)")
        onLog("[INFO] Opening APK archive")
        guard let archive = Archive(url: url, accessMode: .read) else {
            onLog("[ERROR] Cannot open APK archive for execution")
            throw RuntimeExecutionError.unreadableArchive
        }

        var manifestData = Data()
        var dexDataList: [Data] = []
        var soDataList: [(String, Data)] = []
        var resourceTableData = Data()

        // XAPK/APKS/APKM imports are normalized to <package>/base.apk and
        // <package>/components/*.apk. Read the base first, then each retained split.
        var packageArchives: [Archive] = [archive]
        let componentsURL = url.deletingLastPathComponent().appendingPathComponent("components", isDirectory: true)
        let splitURLs = (try? FileManager.default.contentsOfDirectory(
            at: componentsURL, includingPropertiesForKeys: nil
        ))?.filter { $0.pathExtension.lowercased() == "apk" } ?? []
        for splitURL in splitURLs.sorted(by: { $0.lastPathComponent < $1.lastPathComponent }) {
            if let splitArchive = Archive(url: splitURL, accessMode: .read) {
                packageArchives.append(splitArchive)
            } else {
                onLog("[WARN] Could not open preserved split APK: \\(splitURL.lastPathComponent)")
            }
        }
        onLog("[INFO] Package components discovered: \\(packageArchives.count) APK(s), \\(splitURLs.count) split(s)")

        for (archiveIndex, packageArchive) in packageArchives.enumerated() {
            for entry in packageArchive {
                if entry.path == "resources.arsc" {
                    // ResourceCatalog currently accepts one table. Prefer the base table.
                    if resourceTableData.isEmpty {
                        _ = try packageArchive.extract(entry) { chunk in resourceTableData.append(chunk) }
                    }
                } else if entry.path == "AndroidManifest.xml" {
                    if archiveIndex == 0 && manifestData.isEmpty {
                        _ = try packageArchive.extract(entry) { chunk in manifestData.append(chunk) }
                    }
                } else if entry.path == "classes.dex" || entry.path.range(of: #"^classes[0-9]+\\.dex$"#, options: .regularExpression) != nil {
                    var dex = Data()
                    _ = try packageArchive.extract(entry) { chunk in dex.append(chunk) }
                    dexDataList.append(dex)
                } else if (entry.path.hasPrefix("lib/arm64-v8a/") || entry.path.hasPrefix("lib/armeabi-v7a/")) && entry.path.hasSuffix(".so") {
                    var so = Data()
                    _ = try packageArchive.extract(entry) { chunk in so.append(chunk) }
                    let name = URL(fileURLWithPath: entry.path).lastPathComponent
                    soDataList.append((name, so))
                }
            }
        }

        onLog("[INFO] Archive scan complete: manifest=\\(!manifestData.isEmpty), manifest_bytes=\\(manifestData.count), DEX=\\(dexDataList.count), native libraries=\\(soDataList.count), APK components=\\(packageArchives.count)")
        guard !manifestData.isEmpty else {
            onLog("[ERROR] AndroidManifest.xml is absent or empty")
            throw RuntimeExecutionError.missingManifest
        }

        let packageName = url.deletingPathExtension().lastPathComponent
        onLog("[INFO] Resource table: \(resourceTableData.count) bytes")
        onLog("[INFO] Initializing ART-compatible runtime")
        guard let runtime = packageName.withCString({ stride_runtime_create($0) }) else {
            onLog("[FATAL] Rust runtime creation returned null")
            throw RuntimeExecutionError.runtimeInitializationFailed
        }
        if !resourceTableData.isEmpty {
            let resourceStatus = resourceTableData.withUnsafeBytes { raw in
                stride_runtime_load_resource_table(runtime, raw.bindMemory(to: UInt8.self).baseAddress, resourceTableData.count)
            }
            if resourceStatus == 0 { onLog("[INFO] Android resource table decoded") }
            else { onLog("[WARN] Resource table decode unavailable (status \(resourceStatus)); literal UI text remains available") }
        }
        if !resourceTableData.isEmpty {
            let fileCount = stride_runtime_resource_file_count(runtime)
            var decodedImages = 0
            for index in 0..<min(fileCount, 4096) {
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
                      }).first,
                      let (resourceArchive, entry) = located else { continue }
                var encoded = Data()
                _ = try resourceArchive.extract(entry) { encoded.append($0) }
                guard let image = UIImage(data: encoded), let cgImage = image.cgImage else { continue }
                let width = cgImage.width, height = cgImage.height
                guard width > 0, height > 0, width * height <= 4_194_304 else { continue }
                var rgba = [UInt8](repeating: 0, count: width * height * 4)
                let rendered = rgba.withUnsafeMutableBytes { raw -> Bool in
                    guard let context = CGContext(data: raw.baseAddress, width: width, height: height,
                        bitsPerComponent: 8, bytesPerRow: width * 4,
                        space: CGColorSpaceCreateDeviceRGB(),
                        bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue) else { return false }
                    context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
                    return true
                }
                guard rendered else { continue }
                var argb = [UInt32](repeating: 0, count: width * height)
                for pixel in 0..<(width * height) {
                    let p = pixel * 4
                    argb[pixel] = (UInt32(rgba[p + 3]) << 24) | (UInt32(rgba[p]) << 16) | (UInt32(rgba[p + 1]) << 8) | UInt32(rgba[p + 2])
                }
                let status = argb.withUnsafeBufferPointer { buffer in
                    stride_runtime_set_resource_image(runtime, resourceID, UInt32(width), UInt32(height), buffer.baseAddress, buffer.count)
                }
                if status == 0 { decodedImages += 1 }
            }
            onLog("[INFO] Decoded \(decodedImages) Android image resources")
        }
        var runtimeOwnershipTransferred = false
        defer {
            if !runtimeOwnershipTransferred {
                stride_runtime_destroy(runtime)
            }
        }

        // A manifest without executable DEX code cannot enter the managed runtime.
        guard !dexDataList.isEmpty else {
            onLog("[ERROR] No classes*.dex entries found in APK")
            throw RuntimeExecutionError.missingDex
        }

        // Load every DEX and stop immediately if the runtime rejects one.
        for (index, dex) in dexDataList.enumerated() {
            onLog("[INFO] Loading DEX file \(index + 1)/\(dexDataList.count) (\(dex.count) bytes)")
            let dexStatus = dex.withUnsafeBytes { rawBuffer -> Int32 in
                guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return 1
                }
                return stride_runtime_load_dex(runtime, ptr, rawBuffer.count)
            }
            guard dexStatus == 0 else {
                onLog("[ERROR] DEX \(index + 1) load failed with status \(dexStatus)")

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
                throw RuntimeExecutionError.dexLoadFailed(index: index, status: dexStatus, detail: detail)
            }
            onLog("[INFO] DEX file \(index + 1) loaded successfully (\(dex.count) bytes)")
        }

        // Load native shared libraries (.so) into the Bionic/ELF linker
        for (soName, soData) in soDataList {
            onLog("[INFO] Loading native library \(soName) (\(soData.count) bytes)")
            let nativeStatus = soData.withUnsafeBytes { rawBuffer -> Int32 in
                guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return 1
                }
                return soName.withCString { namePtr in
                    stride_runtime_load_so(runtime, namePtr, ptr, rawBuffer.count)
                }
            }
            guard nativeStatus == 0 else {
                // Best-effort launch: a failed optional/unsupported .so must not
                // prevent trying the remaining libraries and managed Activity.
                // Apps that require this library may still fail later at the call site.
                onLog("[WARN] Native library \(soName) could not be loaded (status \(nativeStatus)); continuing best-effort execution")
                continue
            }
            onLog("[INFO] Native library \(soName) loaded successfully")
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
        onLog("[WARN] Guest frame scheduler and Android event-loop dispatch are not active yet")
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
