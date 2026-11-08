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

struct APKExecutionResult: Equatable {
    let statusCode: Int32
    let totalClassesLoaded: UInt32
    let instructionsExecuted: UInt32
    let lifecycleState: String
    let missingApisCount: UInt32
    let averageFps: Float
    let heapAllocatedBytes: UInt32
}

enum RuntimeExecutionError: LocalizedError {
    case unreadableArchive
    case missingManifest
    case missingDex
    case dexLoadFailed(index: Int, status: Int32)
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
        case .dexLoadFailed(let index, let status):
            return "Failed to load DEX file #\(index + 1) (runtime status \(status))."
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
        guard !manifestData.isEmpty else {
            throw RuntimeExecutionError.missingManifest
        }

        let packageName = url.deletingPathExtension().lastPathComponent
        onLog("[INFO] Initializing ART-compatible runtime")
        guard let runtime = packageName.withCString({ stride_runtime_create($0) }) else {
            throw RuntimeExecutionError.runtimeInitializationFailed
        }
        defer {
            stride_runtime_destroy(runtime)
        }

        // A manifest without executable DEX code cannot enter the managed runtime.
        guard !dexDataList.isEmpty else {
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
                throw RuntimeExecutionError.dexLoadFailed(index: index, status: dexStatus)
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

        onLog("[INFO] Runtime returned status=\(report.status_code), lifecycle=\(report.lifecycle_state), classes=\(report.total_classes_loaded), instructions=\(report.instructions_executed), missing APIs=\(report.missing_apis_count)")
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

        onLog("[INFO] Lifecycle launch attempt returned. This is not yet a persistent Android app session.")
        return APKExecutionResult(
            statusCode: report.status_code,
            totalClassesLoaded: report.total_classes_loaded,
            instructionsExecuted: report.instructions_executed,
            lifecycleState: lifecycleDescription,
            missingApisCount: report.missing_apis_count,
            averageFps: report.average_fps,
            heapAllocatedBytes: report.heap_allocated_bytes
        )
    }
}
