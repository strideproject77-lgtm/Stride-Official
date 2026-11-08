import Foundation
import ZIPFoundation
import StrideDexFFI

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

struct APKExecutionResult {
    let statusCode: Int32
    let totalClassesLoaded: UInt32
    let instructionsExecuted: UInt32
    let lifecycleState: String
    let missingApisCount: UInt32
    let averageFps: Float
    let heapAllocatedBytes: UInt32
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
        StrideDiagnostics.recordAsync(severity: "INFO", stage: "ARCHIVE", code: "OPEN", message: "Opening APK \(url.lastPathComponent)")
        onLog("[INFO] Opening APK archive")
        guard let archive = Archive(url: url, accessMode: .read) else {
            throw RuntimeExecutionError.unreadableArchive
        }

        var manifestData = Data()
        var dexDataList: [Data] = []
        var soDataList: [(String, Data)] = []

        for entry in archive {
            if entry.path == "AndroidManifest.xml" {
                _ = try archive.extract(entry) { chunk in
                    manifestData.append(chunk)
                }
            } else if entry.path == "classes.dex" || entry.path.range(of: #"^classes[0-9]+\.dex$"#, options: .regularExpression) != nil {
                var dex = Data()
                _ = try archive.extract(entry) { chunk in
                    dex.append(chunk)
                }
                dexDataList.append(dex)
            } else if (entry.path.hasPrefix("lib/arm64-v8a/") || entry.path.hasPrefix("lib/armeabi-v7a/")) && entry.path.hasSuffix(".so") {
                var so = Data()
                _ = try archive.extract(entry) { chunk in
                    so.append(chunk)
                }
                let name = URL(fileURLWithPath: entry.path).lastPathComponent
                soDataList.append((name, so))
            }
        }

        onLog("[INFO] Archive scan complete: manifest=\(!manifestData.isEmpty), DEX=\(dexDataList.count), native libraries=\(soDataList.count)")
        StrideDiagnostics.recordAsync(severity: "INFO", stage: "ARCHIVE", code: "INVENTORY", message: "manifest=\(!manifestData.isEmpty), DEX=\(dexDataList.count), native libraries=\(soDataList.count)")
        guard !manifestData.isEmpty else {
            StrideDiagnostics.recordAsync(severity: "ERROR", stage: "ARCHIVE", code: "MISSING_MANIFEST", message: "AndroidManifest.xml is absent or empty", recovery: "Re-export a valid Android APK.")
            throw RuntimeExecutionError.missingManifest
        }

        let packageName = url.deletingPathExtension().lastPathComponent
        onLog("[INFO] Initializing ART-compatible runtime")
        guard let runtime = packageName.withCString({ stride_runtime_create($0) }) else {
            StrideDiagnostics.recordAsync(severity: "FATAL", stage: "RUNTIME", code: "CREATE_FAILED", message: "Rust runtime creation returned null")
            throw RuntimeExecutionError.runtimeInitializationFailed
        }
        var runtimeOwnershipTransferred = false
        defer {
            if !runtimeOwnershipTransferred {
                stride_runtime_destroy(runtime)
            }
        }

        // A manifest without executable DEX code cannot enter the managed runtime.
        guard !dexDataList.isEmpty else {
            StrideDiagnostics.recordAsync(severity: "ERROR", stage: "DEX", code: "MISSING_DEX", message: "No classes*.dex entries found in APK")
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
                StrideDiagnostics.recordAsync(severity: "ERROR", stage: "ELF", code: "LOAD_FAILED", message: "\(soName) load failed with status \(nativeStatus)", recovery: "Inspect ELF ABI, relocations and unresolved dependencies in the runtime log.")
                onLog("[ERROR] Native library \(soName) load failed with status \(nativeStatus)")
                throw RuntimeExecutionError.nativeLibraryLoadFailed(name: soName, status: nativeStatus)
            }
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
            StrideDiagnostics.recordAsync(severity: "FATAL", stage: "FFI", code: "LAUNCH_BRIDGE_FAILED", message: "Runtime launch bridge returned status \(status)")
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

        onLog("[INFO] Runtime returned status=\(report.status_code), lifecycle=\(report.lifecycle_state), classes=\(report.total_classes_loaded), instructions=\(report.instructions_executed), missing APIs=\(report.missing_apis_count)")
        if report.status_code == 2 {
            StrideDiagnostics.recordAsync(severity: "FATAL", stage: "LIFECYCLE", code: "UNCAUGHT_EXCEPTION", message: "Uncaught Activity lifecycle exception")
            onLog("[CRASH] Runtime reported an uncaught Activity lifecycle exception")
            throw RuntimeExecutionError.uncaughtException("Runtime reported uncaught exception during Activity lifecycle")
        } else if report.status_code == 3 {
            StrideDiagnostics.recordAsync(severity: "ERROR", stage: "LIFECYCLE", code: "ACTIVITY_NOT_FOUND", message: "Launcher Activity was not found")
            onLog("[ERROR] Launcher Activity was not found")
            throw RuntimeExecutionError.activityNotFound("Launcher Activity not found in DEX")
        } else if report.status_code == 4 {
            StrideDiagnostics.recordAsync(severity: "ERROR", stage: "MANIFEST", code: "BINARY_XML_FAILED", message: "Binary Android manifest parsing failed", recovery: "Check AXML chunk sizes, string pool offsets and manifest encoding.")
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
            session: session
        )
    }
}
