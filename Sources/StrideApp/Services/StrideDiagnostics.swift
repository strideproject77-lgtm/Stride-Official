import Foundation
import CryptoKit
import Combine
import os
import UIKit

/// Unified append-only diagnostics shared by import, preflight and runtime execution.
@MainActor
final class StrideDiagnostics: ObservableObject {
    static let shared = StrideDiagnostics()

    enum Severity: String, Sendable, Codable {
        case info = "INFO"
        case warning = "WARN"
        case error = "ERROR"
        case fatal = "FATAL"
    }

    struct Entry: Identifiable, Codable {
        let id: UUID
        let timestamp: Date
        let sequence: UInt64
        let sessionID: String
        let uptimeSeconds: TimeInterval
        let severity: Severity
        let stage: String
        let code: String
        let message: String
        let recovery: String?
        var line: String {
            let stamp = ISO8601DateFormatter().string(from: timestamp)
            let suffix = recovery.map { " | recovery: \($0)" } ?? ""
            return "[\(stamp)] [#\(sequence)] [S:\(sessionID.prefix(8))] [\(severity.rawValue)] [\(stage)] [\(code)] \(message)\(suffix)"
        }
    }

    @Published private(set) var entries: [Entry] = []
    @Published private(set) var currentAPK: String?
    @Published private(set) var issueCount = 0
    @Published private(set) var sessionID = UUID().uuidString

    private let logger = Logger(subsystem: "com.strideproject.stride", category: "diagnostics")
    private let fileManager = FileManager.default
    private var handle: FileHandle?
    private var structuredHandle: FileHandle?
    private var sessionHandle: FileHandle?
    private var pendingText = Data()
    private var pendingStructured = Data()
    private var pendingSession = Data()
    private var sessionFileURL: URL?
    private let flushBatchSize = 16
    private var sequence: UInt64 = 0
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

    var structuredLogFileURL: URL {
        documentsURL.appendingPathComponent("stride-diagnostics.jsonl")
    }

    var currentSessionLogURL: URL? { sessionFileURL }

    func latestSessionLogURL(apkName: String) -> URL? {
        let folder = documentsURL.appendingPathComponent("StrideLogs/Sessions", isDirectory: true)
        guard let files = try? fileManager.contentsOfDirectory(at: folder, includingPropertiesForKeys: [.contentModificationDateKey]) else { return nil }
        let safeName = apkName.replacingOccurrences(of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression)
        return files.filter { $0.lastPathComponent.hasPrefix(String(safeName.prefix(72)) + "-") && $0.pathExtension == "log" }
            .sorted {
                let a = (try? $0.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let b = (try? $1.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                return a > b
            }.first
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
        if sessionHandle != nil { finishSession(status: "replaced by a new launch") }
        currentAPK = apkName
        issueCount = 0
        sequence = 0
        pendingSession.removeAll(keepingCapacity: true)
        entries.removeAll(keepingCapacity: true)
        sessionID = UUID().uuidString
        sessionStartedAt = Date()
        do {
            let folder = documentsURL.appendingPathComponent("StrideLogs/Sessions", isDirectory: true)
            try fileManager.createDirectory(at: folder, withIntermediateDirectories: true)
            let safeName = apkName.replacingOccurrences(of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression)
            let file = folder.appendingPathComponent("\(safeName.prefix(72))-\(sessionID.prefix(8)).log")
            _ = fileManager.createFile(atPath: file.path, contents: nil)
            sessionHandle = try FileHandle(forWritingTo: file)
            sessionFileURL = file
        } catch {
            record(.error, stage: "SESSION", code: "SESSION_LOG_OPEN_FAILED",
                   "Could not open per-session log: \(error.localizedDescription)",
                   recovery: "Global diagnostics remain active; check app storage.")
        }
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
        sequence &+= 1
        let entry = Entry(id: UUID(), timestamp: Date(), sequence: sequence, sessionID: sessionID,
                          uptimeSeconds: ProcessInfo.processInfo.systemUptime,
                          severity: severity, stage: stage, code: code,
                          message: message, recovery: recovery)
        entries.append(entry)
        if entries.count > capacity { entries.removeFirst(entries.count - capacity) }
        if severity == .error || severity == .fatal { issueCount += 1 }
        logger.log("\(entry.line, privacy: .public)")
        guard let data = (entry.line + "\n").data(using: .utf8) else { return }
        pendingText.append(data)
        pendingSession.append(data)
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        if let structured = try? encoder.encode(entry) {
            pendingStructured.append(structured)
            pendingStructured.append(0x0A)
        }
        if severity != .info || sequence % UInt64(flushBatchSize) == 0 { flush() }
    }

    func finishSession(status: String) {
        record(.info, stage: "SESSION", code: "END",
               "Session ended: \(status); collected \(issueCount) error(s)")
        flush()
        try? sessionHandle?.close()
        sessionHandle = nil
        sessionFileURL = nil
        try? fileManager.removeItem(at: markerURL)
    }


    /// Batch routine events to reduce disk I/O; flush on warnings, failures and app backgrounding.
    func flush() {
        writeBuffered(&pendingText, to: handle, label: "text")
        writeBuffered(&pendingStructured, to: structuredHandle, label: "structured")
        writeBuffered(&pendingSession, to: sessionHandle, label: "session")
        do {
            try handle?.synchronize()
            try structuredHandle?.synchronize()
            try sessionHandle?.synchronize()
        } catch {
            logger.error("Could not synchronize diagnostic logs: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func writeBuffered(_ data: inout Data, to file: FileHandle?, label: String) {
        guard !data.isEmpty, let file else { return }
        do {
            try file.write(contentsOf: data)
            data.removeAll(keepingCapacity: true)
        } catch {
            logger.error("Could not append \(label, privacy: .public) diagnostics: \(error.localizedDescription, privacy: .public)")
        }
    }

    /// Creates a portable, per-APK report while preserving the unmodified raw session log.
    func exportSessionReport(apkName: String, apkURL: URL, status: String,
                             rawLog: String, packageName: String = "Not resolved",
                             inspection: APKInspection? = nil,
                             packageFormat: String = "APK", splitCount: Int = 0,
                             runtimeMetrics: String = "Not captured") -> URL? {
        do {
            try fileManager.createDirectory(at: reportsFolder, withIntermediateDirectories: true)
            let attrs = try? fileManager.attributesOfItem(atPath: apkURL.path)
            let fileSize = (attrs?[.size] as? NSNumber)?.int64Value ?? -1
            let appVersion = Bundle.main.infoDictionary?["CFBundleShortVersionString"] as? String ?? "unknown"
            let appBuild = Bundle.main.infoDictionary?["CFBundleVersion"] as? String ?? "unknown"
            let duration = max(0, Date().timeIntervalSince(sessionStartedAt))
            let apkSHA256 = sha256(of: apkURL) ?? "Unavailable (file could not be read)"
            flush()
            let persistedSessionLog = sessionFileURL.flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? rawLog
            let lines = persistedSessionLog.components(separatedBy: .newlines).filter { !$0.isEmpty }
            let findings = classify(lines)
            let resolvedPackage = packageName != "Not resolved" ? packageName :
                (lines.first(where: { $0.contains("Android package identity:") })?
                    .components(separatedBy: "Android package identity:").last?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? "Not resolved")
            let runtimeTimeline = lines.filter {
                $0.contains("[RUNTIME]") || $0.localizedCaseInsensitiveContains("missing api") ||
                $0.localizedCaseInsensitiveContains("unresolved method") ||
                $0.localizedCaseInsensitiveContains("unlinked class")
            }
            let stamp = ISO8601DateFormatter().string(from: sessionStartedAt)
            let safeName = apkName.replacingOccurrences(of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression)
            let url = reportsFolder.appendingPathComponent("\(safeName)-\(sessionID.prefix(8))-Crash-Report.txt")

            var report = """
            STRIDE SESSION / CRASH REPORT
            ========================================
            Session ID: \(sessionID)
            Result: \(status)
            APK: \(apkName)
            Package: \(resolvedPackage)
            Package format: \(packageFormat)
            Split components: \(splitCount)
            APK size: \(fileSize >= 0 ? ByteCountFormatter.string(fromByteCount: fileSize, countStyle: .file) : "Unavailable")
            APK SHA-256: \(apkSHA256)
            Archive entries: \(inspection?.archiveEntryCount.description ?? "Not captured")
            Manifest present: \(inspection?.hasAndroidManifest.description ?? "Not captured")
            DEX files: \(inspection?.dexFileCount.description ?? "Not captured") (analyzed: \(inspection?.analyzedDexFileCount.description ?? "Not captured"), skipped: \(inspection?.skippedDexFileCount.description ?? "Not captured"))
            DEX inventory: \(inspection.map { "\($0.dexClassCount) classes / \($0.dexMethodCount) method references" } ?? "Not captured")
            Native ABIs: \(inspection?.nativeABIs.joined(separator: ", ") ?? "Not captured")
            Resource table: \(inspection?.hasResourceTable.description ?? "Not captured")
            Started: \(stamp)
            Duration: \(String(format: "%.1f", duration)) seconds
            Stride version: \(appVersion) (build \(appBuild))
            Device: \(UIDevice.current.model)
            OS: iOS \(UIDevice.current.systemVersion)
            Physical memory: \(ByteCountFormatter.string(fromByteCount: Int64(ProcessInfo.processInfo.physicalMemory), countStyle: .memory))
            Runtime metrics: \(runtimeMetrics)
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
            report += "\n\nRUNTIME DIAGNOSTIC EXTRACT (all matching lines; raw log remains authoritative)\n========================================\n"
            report += runtimeTimeline.isEmpty ? "(No runtime diagnostic markers were captured.)\n" : runtimeTimeline.joined(separator: "\n") + "\n"
            report += "\nIMPORTANT INTERPRETATION NOTES\n"
            report += "- A missing end marker on next launch means possible previous-run termination only; it does not prove a crash or jetsam.\n"
            report += "- Unsupported syscalls and unresolved imports are compatibility findings, not proof of the crash cause by themselves.\n"
            report += "- Available-memory samples, virtual address-space mappings, and physical memory are different measurements.\n"
            report += "- Native signal/register/backtrace details appear only if the runtime supplied them; this report does not install a competing signal handler.\n"
            report += "\nFULL RAW SESSION LOG (preserved)\n========================================\n"
            report += persistedSessionLog.isEmpty ? "(No raw session lines were captured.)\n" : persistedSessionLog + (persistedSessionLog.hasSuffix("\n") ? "" : "\n")
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

    private func sha256(of url: URL) -> String? {
        guard let file = try? FileHandle(forReadingFrom: url) else { return nil }
        defer { try? file.close() }
        var hasher = SHA256()
        do {
            while let chunk = try file.read(upToCount: 1_048_576), !chunk.isEmpty {
                hasher.update(data: chunk)
            }
            return hasher.finalize().map { String(format: "%02x", $0) }.joined()
        } catch {
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
            ("OUT_OF_MEMORY_EVIDENCE", { $0.localizedCaseInsensitiveContains("out of memory") || $0.localizedCaseInsensitiveContains("jetsam") }),
            ("MISSING_ANDROID_API_OR_CLASS", { $0.localizedCaseInsensitiveContains("missing_api") || $0.localizedCaseInsensitiveContains("unresolved method") || $0.localizedCaseInsensitiveContains("unlinked class") }),
            ("DEX_OR_INTERPRETER_FAILURE", { $0.localizedCaseInsensitiveContains("unsupported opcode") || $0.localizedCaseInsensitiveContains("instruction budget") || $0.localizedCaseInsensitiveContains("verifier") }),
            ("JNI_BRIDGE_FAILURE", { $0.localizedCaseInsensitiveContains("jni") && ($0.localizedCaseInsensitiveContains("failed") || $0.localizedCaseInsensitiveContains("exception") || $0.localizedCaseInsensitiveContains("unresolved")) }),
            ("NATIVE_DEPENDENCY_FAILURE", { $0.localizedCaseInsensitiveContains("dt_needed") || $0.localizedCaseInsensitiveContains("unresolved dependency") || $0.localizedCaseInsensitiveContains("native library load failed") }),
            ("GRAPHICS_OR_RENDERING", { ($0.localizedCaseInsensitiveContains("egl") || $0.localizedCaseInsensitiveContains("gles") || $0.localizedCaseInsensitiveContains("surface")) && ($0.localizedCaseInsensitiveContains("failed") || $0.localizedCaseInsensitiveContains("unsupported")) }),
            ("INPUT_OR_LIFECYCLE_FAILURE", { ($0.localizedCaseInsensitiveContains("touch") || $0.localizedCaseInsensitiveContains("activity") || $0.localizedCaseInsensitiveContains("lifecycle")) && ($0.localizedCaseInsensitiveContains("failed") || $0.localizedCaseInsensitiveContains("exception")) })
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

    private func rotateLogIfNeeded(_ url: URL) {
        guard let attributes = try? fileManager.attributesOfItem(atPath: url.path),
              let size = attributes[.size] as? NSNumber,
              size.int64Value > 8 * 1024 * 1024 else { return }
        let archiveFolder = documentsURL.appendingPathComponent("StrideLogs/Archive", isDirectory: true)
        do {
            try fileManager.createDirectory(at: archiveFolder, withIntermediateDirectories: true)
            let suffix = Int(Date().timeIntervalSince1970)
            let archived = archiveFolder.appendingPathComponent("\(url.deletingPathExtension().lastPathComponent)-\(suffix).\(url.pathExtension)")
            try fileManager.moveItem(at: url, to: archived)
        } catch {
            logger.error("Could not rotate diagnostics file: \(error.localizedDescription, privacy: .public)")
        }
    }

    private func openLog() {
        rotateLogIfNeeded(logFileURL)
        rotateLogIfNeeded(structuredLogFileURL)
        let url = logFileURL
        if !fileManager.fileExists(atPath: url.path) {
            fileManager.createFile(atPath: url.path, contents: nil)
        }
        do {
            handle = try FileHandle(forWritingTo: url)
            try handle?.seekToEnd()
            if !fileManager.fileExists(atPath: structuredLogFileURL.path) {
                _ = fileManager.createFile(atPath: structuredLogFileURL.path, contents: nil)
            }
            structuredHandle = try FileHandle(forWritingTo: structuredLogFileURL)
            try structuredHandle?.seekToEnd()
        } catch {
            logger.error("Could not open diagnostics file: \(error.localizedDescription, privacy: .public)")
        }
    }
}
