#if STRIDE_DEVELOPER_MODE
import Foundation
import SwiftUI
import UIKit

struct DeveloperBatchTestRow: Identifiable {
    enum State: String {
        case queued = "Queued"
        case testing = "Testing"
        case readyForManualCheck = "Ready for manual check"
        case partial = "Partial evidence"
        case blocked = "Blocked"
        case snapshotComplete = "Snapshot complete"
        case cancelled = "Cancelled"
    }

    let id: UUID
    let package: ImportedAPK
    var progress: Int
    var state: State
    var detail: String
    var logLines: [String]

    init(package: ImportedAPK) {
        self.id = package.id
        self.package = package
        self.progress = 0
        self.state = .queued
        self.detail = "Waiting for test slot"
        self.logLines = []
    }
}

/// Runs one guest runtime at a time. iOS does not permit Stride to keep arbitrary
/// app work executing after the host is suspended, so the test dashboard must stay
/// foregrounded while the queue is active.
@MainActor
final class DeveloperBatchTestRunner: ObservableObject {
    @Published private(set) var rows: [DeveloperBatchTestRow] = []
    @Published private(set) var isRunning = false
    @Published private(set) var activeAppName: String?
    @Published private(set) var previewImage: UIImage?
    @Published private(set) var reportParts: [String] = []
    @Published private(set) var reportPartURLs: [URL] = []
    @Published private(set) var fullReportURL: URL?
    @Published private(set) var finishedCount = 0
    @Published private(set) var overallLog: [String] = []

    private var runTask: Task<Void, Never>?
    private let observationWindowNanoseconds: UInt64 = 20_000_000_000
    private let pollIntervalNanoseconds: UInt64 = 300_000_000
    private let promptFriendlyPartCharacterLimit = 9_000

    var overallProgress: Int {
        guard !rows.isEmpty else { return 0 }
        return Int((Double(finishedCount) / Double(rows.count) * 100).rounded())
    }

    func start(packages: [ImportedAPK]) {
        guard !isRunning, !packages.isEmpty else { return }
        rows = packages.map(DeveloperBatchTestRow.init(package:))
        finishedCount = 0
        activeAppName = nil
        previewImage = nil
        reportParts = []
        reportPartURLs = []
        fullReportURL = nil
        overallLog = [
            "STRIDE DEVELOPER MODE — BATCH RUNTIME TEST",
            "Started: \(ISO8601DateFormatter().string(from: Date()))",
            "Imported apps: \(packages.count)",
            "Execution policy: sequential guest sessions; one runtime at a time",
            "Observation window per app: 20 seconds unless a fatal runtime signal is observed",
            "Interpretation: a completed observation window is not proof of playable gameplay.",
            ""
        ]
        isRunning = true
        runTask = Task { [weak self] in
            guard let self else { return }
            await self.runQueue()
        }
    }

    func stop() {
        runTask?.cancel()
        runTask = nil
        if isRunning {
            for index in rows.indices where rows[index].state == .queued || rows[index].state == .testing {
                rows[index].state = .cancelled
                rows[index].detail = "Stopped by user; no readiness claim"
            }
            appendOverall("[QUEUE] Stopped by user.")
            isRunning = false
            activeAppName = nil
            previewImage = nil
            buildReportFiles()
        }
    }

    func copyPart(at index: Int) {
        guard reportParts.indices.contains(index) else { return }
        UIPasteboard.general.string = reportParts[index]
    }

    private func runQueue() async {
        for index in rows.indices {
            if Task.isCancelled { break }
            await runOne(at: index)
            finishedCount = rows.filter {
                $0.state != .queued && $0.state != .testing
            }.count
        }

        if Task.isCancelled {
            for index in rows.indices where rows[index].state == .queued {
                rows[index].state = .cancelled
                rows[index].detail = "Queue cancelled before launch"
            }
        }
        appendOverall("")
        appendOverall("QUEUE SUMMARY: \(finishedCount)/\(rows.count) apps received a result or cancellation state.")
        appendOverall("A 100% runtime readiness score means launch/lifecycle/instruction/frame checks passed; it is not a claim of full gameplay compatibility.")
        buildReportFiles()
        isRunning = false
        activeAppName = nil
        previewImage = nil
        runTask = nil
    }

    private func runOne(at index: Int) async {
        let package = rows[index].package
        activeAppName = package.displayName
        previewImage = nil
        rows[index].state = .testing
        rows[index].detail = "Opening package and initializing runtime"
        rows[index].progress = 5
        rows[index].logLines = []
        appendAppLog(index, "============================================================")
        appendAppLog(index, "APK: \(package.displayName)")
        appendAppLog(index, "Package path: \(package.fileURL.lastPathComponent)")
        appendAppLog(index, "Started: \(ISO8601DateFormatter().string(from: Date()))")
        appendAppLog(index, "Package format: \(package.packageFormat.rawValue)")
        appendAppLog(index, "Split APK count: \(package.splitAPKURLs.count)")
        appendAppLog(index, "Native ABIs found by import inspection: \(package.inspection.nativeABIs.joined(separator: ", ").isEmpty ? "none" : package.inspection.nativeABIs.joined(separator: ", "))")

        var runtimeSummary = "No runtime result returned."
        var finalState: DeveloperBatchTestRow.State = .blocked
        var finalProgress = 0
        var session: APKRuntimeSession?

        do {
            try Task.checkCancellation()
            let backend = StrideArtExecutionBackend()
            let result = try await backend.launch(packageAt: package.fileURL) { [weak self] line in
                Task { @MainActor [weak self] in
                    self?.appendAppLog(index, line)
                }
            }
            session = result.session
            if Task.isCancelled {
                result.session.close()
                throw CancellationError()
            }

            rows[index].progress = 30
            rows[index].detail = "DEX and Activity launch result received"
            appendAppLog(index, "Launch status code: \(result.statusCode)")
            appendAppLog(index, "Lifecycle state: \(result.lifecycleState)")
            appendAppLog(index, "DEX classes loaded: \(result.totalClassesLoaded)")
            appendAppLog(index, "Guest instructions executed at launch: \(result.instructionsExecuted)")
            appendAppLog(index, "Missing Android APIs: \(result.missingApisCount)")

            let deadline = Date().addingTimeInterval(Double(observationWindowNanoseconds) / 1_000_000_000)
            var latestSnapshot = ""
            var latestFrameCount: UInt64 = 0
            var sawFatalSignal = false
            var sawFramebuffer = false

            while Date() < deadline && !Task.isCancelled {
                let pump = result.session.pumpGuestEvents(maxEvents: 12, instructionBudget: 30_000)
                if pump.status != 0 {
                    appendAppLog(index, "[ERROR] Guest event pump returned status \(pump.status)")
                }

                let stats = result.session.guestFrameStats()
                latestFrameCount = stats.count
                if let frame = result.session.copyFramebuffer() {
                    if let cgImage = makeCGImage(data: frame.data, width: frame.width, height: frame.height) {
                        previewImage = UIImage(cgImage: cgImage)
                        sawFramebuffer = true
                    }
                }

                if let snapshot = result.session.diagnosticsSnapshot(), snapshot != latestSnapshot {
                    latestSnapshot = snapshot
                    let lines = snapshot.components(separatedBy: .newlines).filter { !$0.isEmpty }
                    for line in lines {
                        if !rows[index].logLines.contains("[RUST] " + line) {
                            appendAppLog(index, "[RUST] " + line)
                        }
                    }
                    sawFatalSignal = containsTerminalFailure(snapshot)
                }

                rows[index].progress = readinessScore(
                    result: result,
                    frameCount: latestFrameCount,
                    hasFramebuffer: sawFramebuffer,
                    snapshot: latestSnapshot,
                    packageHasNativeCode: !package.inspection.nativeABIs.isEmpty
                )
                rows[index].detail = progressDescription(
                    result: result,
                    frameCount: latestFrameCount,
                    hasFramebuffer: sawFramebuffer,
                    fatal: sawFatalSignal
                )

                if sawFatalSignal {
                    appendAppLog(index, "[TEST] Terminal runtime failure marker detected; ending this app's observation.")
                    break
                }
                try await Task.sleep(nanoseconds: pollIntervalNanoseconds)
            }

            if Task.isCancelled {
                result.session.close()
                throw CancellationError()
            }

            let finalStats = result.session.guestFrameStats()
            let finalSnapshot = result.session.diagnosticsSnapshot() ?? latestSnapshot
            appendAppLog(index, "Final guest frame count: \(finalStats.count)")
            appendAppLog(index, "Final guest FPS: \(finalStats.fps.map { String(format: "%.2f", $0) } ?? "not reported")")
            appendAppLog(index, "Final runtime diagnostics snapshot:")
            appendAppLog(index, finalSnapshot.isEmpty ? "(No runtime snapshot available)" : finalSnapshot)
            appendAppLog(index, "Observation ended: \(ISO8601DateFormatter().string(from: Date()))")

            finalProgress = readinessScore(
                result: result,
                frameCount: finalStats.count,
                hasFramebuffer: sawFramebuffer,
                snapshot: finalSnapshot,
                packageHasNativeCode: !package.inspection.nativeABIs.isEmpty
            )

            if sawFatalSignal || result.statusCode != 0 || result.instructionsExecuted == 0 {
                finalState = .blocked
                runtimeSummary = "Runtime stopped at a measured blocker; inspect this app's section in the combined report."
            } else if finalProgress == 100 {
                finalState = .readyForManualCheck
                runtimeSummary = "Runtime launch checks passed. Manual gameplay/input verification is still required."
            } else {
                finalState = .snapshotComplete
                runtimeSummary = "Observation window completed; this is a bounded snapshot, not proof the app reached a terminal state."
            }
            result.session.close()
            session = nil
        } catch is CancellationError {
            session?.close()
            finalState = .cancelled
            finalProgress = rows[index].progress
            runtimeSummary = "Test cancelled before a complete observation."
            appendAppLog(index, "[TEST] Cancelled.")
        } catch {
            session?.close()
            finalState = .blocked
            finalProgress = 0
            runtimeSummary = error.localizedDescription
            appendAppLog(index, "[FATAL] \(error.localizedDescription)")
        }

        rows[index].progress = finalProgress
        rows[index].state = finalState
        rows[index].detail = runtimeSummary
        appendAppLog(index, "RESULT: \(finalState.rawValue)")
        appendAppLog(index, "RUNTIME READINESS SCORE: \(finalProgress)%")
        appendOverall("")
        appendOverall("----- APP RESULT: \(package.displayName) -----")
        overallLog.append(contentsOf: rows[index].logLines)
        appendOverall("")
        previewImage = nil
    }

    private func readinessScore(
        result: APKExecutionResult,
        frameCount: UInt64,
        hasFramebuffer: Bool,
        snapshot: String,
        packageHasNativeCode: Bool
    ) -> Int {
        guard result.statusCode == 0 else { return 0 }
        var score = 20 // Runtime returned a structured launch result.
        if result.totalClassesLoaded > 0 { score += 10 }
        let lifecycle = result.lifecycleState.lowercased()
        if lifecycle.contains("resume") { score += 20 }
        else if lifecycle.contains("start") || lifecycle.contains("create") { score += 10 }
        if result.instructionsExecuted > 0 { score += 15 }
        if frameCount > 0 && hasFramebuffer { score += 20 }
        if result.missingApisCount == 0 { score += 10 }
        let lower = snapshot.lowercased()
        if !containsTerminalFailure(snapshot) { score += 5 }
        if packageHasNativeCode {
            let nativeEvidence = lower.contains("jni_onload") && (
                lower.contains("completed") || lower.contains("executed") || lower.contains("success")
            )
            if !nativeEvidence { score = min(score, 90) }
        }
        if result.instructionsExecuted == 0 || frameCount == 0 || !hasFramebuffer || result.missingApisCount > 0 {
            score = min(score, 95)
        }
        return min(100, score)
    }

    private func progressDescription(
        result: APKExecutionResult,
        frameCount: UInt64,
        hasFramebuffer: Bool,
        fatal: Bool
    ) -> String {
        if fatal { return "Terminal runtime error captured" }
        if frameCount > 0 && hasFramebuffer { return "Guest frame output observed · \(frameCount) frame(s)" }
        if result.instructionsExecuted > 0 { return "Guest instructions executed · waiting for rendered frame" }
        return "Activity launch returned · waiting for guest execution evidence"
    }

    private func containsTerminalFailure(_ snapshot: String) -> Bool {
        let upper = snapshot.uppercased()
        let markers = [
            "[FATAL]", "NATIVE_CRASH", "UNCAUGHT_EXCEPTION",
            "GUEST_RUNTIME_TERMINATED", "RUNTIME_PANIC", "SIGSEGV", "SIGABRT"
        ]
        return markers.contains { upper.contains($0) }
    }

    private func makeCGImage(data: Data, width: Int, height: Int) -> CGImage? {
        guard width > 0, height > 0,
              let provider = CGDataProvider(data: data as CFData) else { return nil }
        return CGImage(
            width: width,
            height: height,
            bitsPerComponent: 8,
            bitsPerPixel: 32,
            bytesPerRow: width * 4,
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGBitmapInfo(rawValue: CGBitmapInfo.byteOrder32Little.rawValue | CGImageAlphaInfo.premultipliedFirst.rawValue),
            provider: provider,
            decode: nil,
            shouldInterpolate: false,
            intent: .defaultIntent
        )
    }

    private func appendAppLog(_ index: Int, _ line: String) {
        guard rows.indices.contains(index) else { return }
        rows[index].logLines.append(line)
    }

    private func appendOverall(_ line: String) {
        overallLog.append(line)
    }

    private func buildReportFiles() {
        let folder = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("StrideDeveloperTests", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
            let stamp = ISO8601DateFormatter().string(from: Date()).replacingOccurrences(of: ":", with: "-")
            let reportText = overallLog.joined(separator: "\n")
            let fullURL = folder.appendingPathComponent("Stride-Batch-Test-\(stamp)-FULL.txt")
            try reportText.write(to: fullURL, atomically: true, encoding: .utf8)
            fullReportURL = fullURL

            let characters = Array(reportText)
            var parts: [String] = []
            var urls: [URL] = []
            var start = 0
            while start < characters.count {
                let end = min(start + promptFriendlyPartCharacterLimit, characters.count)
                let partNumber = parts.count + 1
                let header = "STRIDE BATCH TEST LOG — PART \(partNumber)\nTotal apps: \(rows.count)\nPart size target: \(promptFriendlyPartCharacterLimit) characters\n\n"
                let part = header + String(characters[start..<end])
                let url = folder.appendingPathComponent("Stride-Batch-Test-\(stamp)-PART-\(String(format: "%03d", partNumber)).txt")
                try part.write(to: url, atomically: true, encoding: .utf8)
                parts.append(part)
                urls.append(url)
                start = end
            }
            if characters.isEmpty {
                let emptyPart = "STRIDE BATCH TEST LOG — PART 1\nNo log content was captured."
                let url = folder.appendingPathComponent("Stride-Batch-Test-\(stamp)-PART-001.txt")
                try emptyPart.write(to: url, atomically: true, encoding: .utf8)
                parts = [emptyPart]
                urls = [url]
            }
            reportParts = parts
            reportPartURLs = urls
            appendOverall("[REPORT] Full report: \(fullURL.lastPathComponent)")
            appendOverall("[REPORT] Prompt-friendly parts generated: \(parts.count)")
        } catch {
            appendOverall("[REPORT ERROR] Could not save combined report: \(error.localizedDescription)")
        }
    }
}

#endif
