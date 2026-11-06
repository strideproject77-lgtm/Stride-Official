import Foundation

/// The execution boundary is intentionally explicit: package inspection is not execution.
protocol AndroidExecutionBackend {
    func launch(packageAt url: URL) async throws
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

/// No backend is registered until a genuine Android execution implementation exists.
struct RuntimeNotImplementedError: LocalizedError {
    var errorDescription: String? {
        "Android execution is not available in this build. APK archive inspection does not launch Android code."
    }
}

struct UnavailableExecutionBackend: AndroidExecutionBackend {
    func launch(packageAt url: URL) async throws {
        throw RuntimeNotImplementedError()
    }
}
