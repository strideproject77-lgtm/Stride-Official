import Foundation
import ZIPFoundation
import StrideDexFFI

/// The execution boundary connecting iOS application requests to the ART managed runtime.
protocol AndroidExecutionBackend {
    func launch(packageAt url: URL) async throws -> APKExecutionResult
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
}

enum RuntimeExecutionError: LocalizedError {
    case unreadableArchive
    case missingManifest
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

    func launch(packageAt url: URL) async throws -> APKExecutionResult {
        guard let archive = Archive(url: url, accessMode: .read) else {
            throw RuntimeExecutionError.unreadableArchive
        }

        var manifestData = Data()
        var dexDataList: [Data] = []

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
            }
        }

        guard !manifestData.isEmpty else {
            throw RuntimeExecutionError.missingManifest
        }

        let packageName = url.deletingPathExtension().lastPathComponent
        guard let runtime = packageName.withCString({ stride_runtime_create($0) }) else {
            throw RuntimeExecutionError.runtimeInitializationFailed
        }
        defer {
            stride_runtime_destroy(runtime)
        }

        // Load all extracted DEX binaries into the runtime
        for dex in dexDataList {
            _ = dex.withUnsafeBytes { rawBuffer in
                if let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) {
                    _ = stride_runtime_load_dex(runtime, ptr, rawBuffer.count)
                }
            }
        }

        var report = StrideLaunchReport()
        let status = manifestData.withUnsafeBytes { rawBuffer -> Int32 in
            guard let ptr = rawBuffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                return 1
            }
            return stride_runtime_launch_apk(runtime, ptr, rawBuffer.count, 200_000, &report)
        }

        guard status == 0 else {
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

        if report.status_code == 2 {
            throw RuntimeExecutionError.uncaughtException("Runtime reported uncaught exception during Activity lifecycle")
        } else if report.status_code == 3 {
            throw RuntimeExecutionError.activityNotFound("Launcher Activity not found in DEX")
        } else if report.status_code == 4 {
            throw RuntimeExecutionError.manifestError("Failed to parse AndroidManifest binary XML")
        }

        return APKExecutionResult(
            statusCode: report.status_code,
            totalClassesLoaded: report.total_classes_loaded,
            instructionsExecuted: report.instructions_executed,
            lifecycleState: lifecycleDescription
        )
    }
}
