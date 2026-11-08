import Foundation
import Combine
import os

/// Stride's unified, append-only diagnostic stream.
/// Keeps a bounded in-memory tail and synchronously persists each line so the
/// last useful message survives an abrupt termination.
@MainActor
final class StrideDiagnostics: ObservableObject {
    static let shared = StrideDiagnostics()

    enum Severity: String {
        case info = "INFO"
        case warning = "WARN"
        case error = "ERROR"
        case fatal = "FATAL"
    }

    struct Entry: Identifiable {
        let id = UUID()
        let timestamp: Date
        let severity: Severity
        let stage: String
        let code: String
        let message: String
        let recovery: String?
        var line: String {
            let time = ISO8601DateFormatter().string(from: timestamp)
            let suffix = recovery.map { " | recovery: \($0)" } ?? ""
            return "[\(time)] [\(severity.rawValue)] [\(stage)] [\(code)] \(message)\(suffix)"
        }
    }

    @Published private(set) var entries: [Entry] = []
    @Published private(set) var currentAPK: String?
    @Published private(set) var issueCount = 0
    @Published private(set) var sessionID = UUID().uuidString

    private let logger = Logger(subsystem: "com.strideproject.stride", category: "diagnostics")
    private let fileManager = FileManager.default
    private var handle: FileHandle?
    private let capacity = 1500

    var logFileURL: URL {
        fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("stride-diagnostics.log")
    }

    private init() {
        openLog()
        record(.info, stage: "BOOT", code: "DIAG_READY",
               "Unified diagnostics initialized; persistent log: stride-diagnostics.log")
    }

    func beginSession(apkName: String) {
        currentAPK = apkName
        issueCount = 0
        sessionID = UUID().uuidString
        record(.info, stage: "SESSION", code: "START",
               "Diagnostic session started for \(apkName)")
    }

    func record(_ severity: Severity, stage: String, code: String,
                _ message: String, recovery: String? = nil) {
        let entry = Entry(timestamp: Date(), severity: severity, stage: stage,
                          code: code, message: message, recovery: recovery)
        entries.append(entry)
        if entries.count > capacity { entries.removeFirst(entries.count - capacity) }
        if severity == .error || severity == .fatal { issueCount += 1 }
        logger.log("\(entry.line, privacy: .public)")
        guard let data = (entry.line + "\n").data(using: .utf8) else { return }
        do {
            try handle?.write(contentsOf: data)
            try handle?.synchronize()
        } catch {
            logger.error("Could not persist diagnostic line: \(error.localizedDescription, privacy: .public)")
        }
    }

    func finishSession(status: String) {
        record(.info, stage: "SESSION", code: "END",
               "Session ended: \(status); collected \(issueCount) error(s)")
        try? handle?.synchronize()
    }

    func clearVisibleEntries() {
        entries.removeAll()
    }

    private func openLog() {
        let url = logFileURL
        if !fileManager.fileExists(atPath: url.path) {
            fileManager.createFile(atPath: url.path, contents: nil)
        }
        do {
            handle = try FileHandle(forWritingTo: url)
            try handle?.seekToEnd()
        } catch {
            logger.error("Could not open diagnostics file: \(error.localizedDescription, privacy: .public)")
        }
    }
}
