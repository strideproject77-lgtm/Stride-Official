import Foundation
import Combine
import os
import UIKit

/// Unified append-only diagnostics shared by import, preflight and runtime execution.
@MainActor
final class StrideDiagnostics: ObservableObject {
    static let shared = StrideDiagnostics()

    enum Severity: String, Sendable {
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
            let stamp = ISO8601DateFormatter().string(from: timestamp)
            let suffix = recovery.map { " | recovery: \($0)" } ?? ""
            return "[\(stamp)] [\(severity.rawValue)] [\(stage)] [\(code)] \(message)\(suffix)"
        }
    }

    @Published private(set) var entries: [Entry] = []
    @Published private(set) var currentAPK: String?
    @Published private(set) var issueCount = 0
    @Published private(set) var sessionID = UUID().uuidString

    private let logger = Logger(subsystem: "com.strideproject.stride", category: "diagnostics")
    private let fileManager = FileManager.default
    private var handle: FileHandle?
    private let capacity = 2000
    private var sessionStartedAt = Date()

    private var documentsURL: URL {
        fileManager.urls(for: .documentDirectory, in: .userDomainMask)[0]
    }

    private var markerURL: URL {
        documentsURL.appendingPathComponent("StrideLogs/active-session.json")
    }

    var logFileURL: URL {
        documentsURL.appendingPathComponent("stride-diagnostics.log")
    }

    private var reportsFolder: URL {
        documentsURL.appendingPathComponent("StrideLogs/Reports", isDirectory: true)
    }

    private init() {
        openLog()
        detectUncleanPreviousSession()
        record(.info, stage: "BOOT", code: "DIAG_READY",
               "Unified diagnostics initialized; persistent log: stride-diagnostics.log")
    }

    nonisolated static func recordAsync(severity: String, stage: String, code: String,
                                        message: String, recovery: String? = nil) {
        Task { @MainActor in
            guard let level = Severity(rawValue: severity) else { return }
            shared.record(level, stage: stage, code: code, message, recovery: recovery)
        }
    }

    func beginSession(apkName: String) {
        currentAPK = apkName
        issueCount = 0
        entries.removeAll(keepingCapacity: true)
        sessionID = UUID().uuidString
        sessionStartedAt = Date()
        let marker: [String: String] = [
            "session_id": sessionID,
            "apk": apkName,
            "started_at": ISO8601DateFormatter().string(from: sessionStartedAt),
            "app_version": Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown",
            "app_build": Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "unknown",
            "ios_version": UIDevice.current.systemVersion
        ]
        do {
            try fileManager.createDirectory(at: markerURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            let data = try JSONSerialization.data(withJSONObject: marker, options: [.prettyPrinted, .sortedKeys])
            try data.write(to: markerURL, options: .atomic)
        } catch {
            record(.error, stage: "SESSION", code: "MARKER_WRITE_FAILED",
                   "Could not persist unclean-session marker: \(error.localizedDescription)")
        }
        record(.info, stage: "SESSION", code: "START", "Diagnostic session started for \(apkName)")
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
        try? fileManager.removeItem(at: markerURL)
    }

    /// Creates a portable, per-APK report while preserving the unmodified raw session log.
    func exportSessionReport(apkName: String, apkURL: URL, status: String,
                             rawLog: String, packageName: String = "Not resolved") -> URL? {
        do {
            try fileManager.createDirectory(at: reportsFolder, withIntermediateDirectories: true)
            let attrs = try? fileManager.attributesOfItem(atPath: apkURL.path)
            let fileSize = (attrs?[.size] as? NSNumber)?.int64Value ?? -1
            let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown"
            let appBuild = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "unknown"
            let duration = max(0, Date().timeIntervalSince(sessionStartedAt))
            let lines = rawLog.components(separatedBy: .newlines).filter { !$0.isEmpty }
            let findings = classify(lines)
            let stamp = ISO8601DateFormatter().string(from: sessionStartedAt)
            let safeName = apkName.replacingOccurrences(of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression)
            let url = reportsFolder.appendingPathComponent("\(safeName)-\(sessionID.prefix(8))-Crash-Report.txt")

            var report = """
            STRIDE SESSION / CRASH REPORT
            ========================================
            Session ID: \(sessionID)
            Result: \(status)
            APK: \(apkName)
            Package: \(packageName)
            APK size: \(fileSize >= 0 ? ByteCountFormatter.string(fromByteCount: fileSize, countStyle: .file) : "Unavailable")
            Started: \(stamp)
            Duration: \(String(format: "%.1f", duration)) seconds
            Stride version: \(appVersion) (build \(appBuild))
            Device: \(UIDevice.current.model)
            OS: iOS \(UIDevice.current.systemVersion)
            Physical memory: \(ByteCountFormatter.string(fromByteCount: Int64(ProcessInfo.processInfo.physicalMemory), countStyle: .memory))
            Retained structured events: \(entries.count)
            Structured errors/fatals: \(issueCount)

            FINDINGS (heuristic classification; raw evidence follows)
            ========================================
            """
            if findings.isEmpty {
                report += "\nNo recognized crash signature was found in the captured log. This does not prove the app exited normally."
            } else {
                for item in findings {
                    report += "\n[\(item.category)] \(item.count)x — \(item.example)"
                }
            }
            report += "\n\nIMPORTANT INTERPRETATION NOTES\n"
            report += "- A missing end marker on next launch means possible previous-run termination only; it does not prove a crash or jetsam.\n"
            report += "- Unsupported syscalls and unresolved imports are compatibility findings, not proof of the crash cause by themselves.\n"
            report += "- Available-memory samples, virtual address-space mappings, and physical memory are different measurements.\n"
            report += "- Native signal/register/backtrace details appear only if the runtime supplied them; this report does not install a competing signal handler.\n"
            report += "\nFULL RAW SESSION LOG (preserved)\n========================================\n"
            report += rawLog.isEmpty ? "(No raw session lines were captured.)\n" : rawLog + (rawLog.hasSuffix("\n") ? "" : "\n")
            report += "\nSTRUCTURED DIAGNOSTIC TIMELINE\n========================================\n"
            report += entries.map(\.line).joined(separator: "\n")
            report += "\n"
            try report.write(to: url, atomically: true, encoding: .utf8)
            record(.info, stage: "REPORT", code: "CRASH_REPORT_EXPORTED",
                   "Saved session report: \(url.lastPathComponent)")
            return url
        } catch {
            record(.error, stage: "REPORT", code: "EXPORT_FAILED",
                   "Could not create session report: \(error.localizedDescription)")
            return nil
        }
    }

    func clearVisibleEntries() {
        entries.removeAll()
    }

    private func classify(_ lines: [String]) -> [(category: String, count: Int, example: String)] {
        let rules: [(String, (String) -> Bool)] = [
            ("NATIVE_CRASH", { $0.contains("FATAL signal") || $0.contains("SIGSEGV") || $0.contains("SIGABRT") || $0.contains("SIGBUS") || $0.contains("SIGILL") || $0.contains("SIGFPE") }),
            ("JAVA_UNCAUGHT_EXCEPTION", { $0.localizedCaseInsensitiveContains("uncaught exception") }),
            ("UNSUPPORTED_SYSCALL", { $0.localizedCaseInsensitiveContains("system call") && ($0.localizedCaseInsensitiveContains("not provided") || $0.localizedCaseInsensitiveContains("ENOSYS")) }),
            ("UNRESOLVED_NATIVE_IMPORT", { $0.localizedCaseInsensitiveContains("unresolved import") }),
            ("RELOCATION_FAILURE", { $0.localizedCaseInsensitiveContains("relocation") && ($0.localizedCaseInsensitiveContains("failed") || $0.localizedCaseInsensitiveContains("unresolved")) }),
            ("NETWORK_TELEMETRY_FAILURE", { $0.localizedCaseInsensitiveContains("firebaselogging") || ($0.localizedCaseInsensitiveContains("connectexception") && $0.localizedCaseInsensitiveContains("443")) }),
            ("RUNTIME_INTERPRETER_FAILURE", { $0.localizedCaseInsensitiveContains("unsupported opcode") || $0.localizedCaseInsensitiveContains("interpreter fault") }),
            ("OUT_OF_MEMORY_EVIDENCE", { $0.localizedCaseInsensitiveContains("out of memory") || $0.localizedCaseInsensitiveContains("jetsam") })
        ]
        var output: [(String, Int, String)] = []
        for (category, matches) in rules {
            let found = lines.filter(matches)
            if let first = found.first {
                output.append((category, found.count, String(first.prefix(320))))
            }
        }
        return output
    }

    private func detectUncleanPreviousSession() {
        guard fileManager.fileExists(atPath: markerURL.path),
              let data = try? Data(contentsOf: markerURL),
              let marker = try? JSONSerialization.jsonObject(with: data) as? [String: String] else { return }
        let apk = marker["apk"] ?? "unknown APK"
        let started = marker["started_at"] ?? "unknown"
        record(.warning, stage: "SESSION", code: "POSSIBLE_PREVIOUS_RUN_TERMINATION",
               "Previous session did not write a clean end marker (APK: \(apk), started: \(started)). Cause is unknown; this is not proof of a crash or jetsam.")
        // Keep the marker until the next explicit run starts, so the evidence is not silently erased.
        let note = reportsFolder.appendingPathComponent("Possible-Previous-Run-Termination.txt")
        try? fileManager.createDirectory(at: reportsFolder, withIntermediateDirectories: true)
        try? "Possible previous-run termination\nAPK: \(apk)\nStarted: \(started)\nNo cause can be inferred from the marker alone.\n".write(to: note, atomically: true, encoding: .utf8)
        try? fileManager.removeItem(at: markerURL)
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
