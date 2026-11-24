#if STRIDE_DEVELOPER_MODE
import SwiftUI
import UIKit

struct DeveloperTestingView: View {
    let packages: [ImportedAPK]
    let accent: Color
    let background: Color
    let panel: Color
    let muted: Color

    @Environment(\.dismiss) private var dismiss
    @StateObject private var runner = DeveloperBatchTestRunner()

    var body: some View {
        NavigationView {
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 16) {
                    VStack(alignment: .leading, spacing: 7) {
                        Text("BATCH RUNTIME TEST")
                            .font(.system(size: 10, weight: .heavy)).tracking(1.4).foregroundStyle(accent)
                        Text("Test every imported APK sequentially and capture real runtime evidence.")
                            .font(.system(size: 12, weight: .medium)).foregroundStyle(muted)
                        Text("Keep Stride open in the foreground. iOS may suspend testing if the app is backgrounded or the screen locks.")
                            .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                    }
                    .padding(15)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))

                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text("\(runner.finishedCount) / \(packages.count) apps processed")
                                .font(.system(size: 15, weight: .bold)).foregroundStyle(.white)
                            Text("Queue progress · \(runner.overallProgress)%")
                                .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                        }
                        Spacer()
                        if runner.isRunning {
                            Button("Stop") { runner.stop() }
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(.white)
                                .padding(.horizontal, 14).padding(.vertical, 9)
                                .background(Color.red.opacity(0.22), in: Capsule())
                        } else {
                            Button {
                                runner.start(packages: packages)
                            } label: {
                                Label("Test All", systemImage: "play.fill")
                                    .font(.system(size: 12, weight: .bold))
                                    .foregroundStyle(Color.black)
                                    .padding(.horizontal, 14).padding(.vertical, 10)
                                    .background(accent, in: Capsule())
                            }
                            .disabled(packages.isEmpty)
                        }
                    }
                    ProgressView(value: Double(runner.overallProgress), total: 100)
                        .tint(accent)

                    if let image = runner.previewImage {
                        VStack(alignment: .leading, spacing: 8) {
                            HStack {
                                Text("LIVE GUEST SCREEN")
                                    .font(.system(size: 9, weight: .heavy)).tracking(1.2).foregroundStyle(accent)
                                Spacer()
                                Text(runner.activeAppName ?? "Current APK")
                                    .font(.system(size: 10, weight: .semibold)).foregroundStyle(muted).lineLimit(1)
                            }
                            Image(uiImage: image)
                                .resizable()
                                .interpolation(.none)
                                .scaledToFit()
                                .frame(maxWidth: .infinity)
                                .frame(height: 210)
                                .background(Color.black, in: RoundedRectangle(cornerRadius: 12))
                                .clipShape(RoundedRectangle(cornerRadius: 12))
                        }
                        .padding(13)
                        .background(panel, in: RoundedRectangle(cornerRadius: 16))
                    } else if runner.isRunning {
                        HStack(spacing: 10) {
                            ProgressView().tint(accent)
                            VStack(alignment: .leading, spacing: 3) {
                                Text(runner.activeAppName ?? "Preparing next APK")
                                    .font(.system(size: 12, weight: .bold)).foregroundStyle(.white)
                                Text("Waiting for a real guest framebuffer.")
                                    .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                            }
                            Spacer()
                        }
                        .padding(14)
                        .background(panel, in: RoundedRectangle(cornerRadius: 14))
                    }

                    VStack(alignment: .leading, spacing: 10) {
                        Text("IMPORTED APPS")
                            .font(.system(size: 9, weight: .heavy)).tracking(1.2).foregroundStyle(muted)
                        if packages.isEmpty {
                            Text("No APKs have been imported yet.")
                                .font(.system(size: 12, weight: .medium)).foregroundStyle(muted)
                        } else {
                            ForEach(runner.rows) { row in
                                VStack(alignment: .leading, spacing: 7) {
                                    HStack(spacing: 9) {
                                        Image(systemName: iconName(for: row.state))
                                            .font(.system(size: 13, weight: .bold))
                                            .foregroundStyle(color(for: row.state, accent: accent))
                                        Text(row.package.displayName)
                                            .font(.system(size: 12, weight: .bold)).foregroundStyle(.white).lineLimit(1)
                                        Spacer(minLength: 4)
                                        Text("\(row.progress)%")
                                            .font(.system(size: 11, weight: .bold, design: .monospaced))
                                            .foregroundStyle(color(for: row.state, accent: accent))
                                    }
                                    ProgressView(value: Double(row.progress), total: 100)
                                        .tint(color(for: row.state, accent: accent))
                                    Text(row.state.rawValue + " · " + row.detail)
                                        .font(.system(size: 9, weight: .medium)).foregroundStyle(muted)
                                        .fixedSize(horizontal: false, vertical: true)
                                }
                                .padding(12)
                                .background(panel, in: RoundedRectangle(cornerRadius: 13))
                            }
                        }
                    }

                    if !runner.reportParts.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("COMBINED COPYABLE LOG")
                                .font(.system(size: 9, weight: .heavy)).tracking(1.2).foregroundStyle(accent)
                            Text("One master report contains a separate section for every APK. It is split into \(runner.reportParts.count) prompt-friendly parts.")
                                .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                            ForEach(Array(runner.reportParts.enumerated()), id: \.offset) { index, part in
                                Button {
                                    runner.copyPart(at: index)
                                } label: {
                                    HStack {
                                        Image(systemName: "doc.on.doc").foregroundStyle(accent)
                                        Text("Copy Part \(index + 1) of \(runner.reportParts.count)")
                                            .font(.system(size: 11, weight: .bold)).foregroundStyle(.white)
                                        Spacer()
                                        Text("\(part.count) chars")
                                            .font(.system(size: 9, weight: .medium, design: .monospaced)).foregroundStyle(muted)
                                    }
                                    .padding(12)
                                    .background(panel, in: RoundedRectangle(cornerRadius: 11))
                                }
                                .buttonStyle(.plain)
                            }
                            if let url = runner.fullReportURL {
                                Text("Full report saved in Stride's Documents: \(url.lastPathComponent)")
                                    .font(.system(size: 9, weight: .medium)).foregroundStyle(muted)
                                    .textSelection(.enabled)
                            }
                        }
                        .padding(13)
                        .background(panel.opacity(0.7), in: RoundedRectangle(cornerRadius: 16))
                    }
                }
                .padding(16)
            }
            .background(background.ignoresSafeArea())
            .navigationTitle("Developer Mode")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { dismiss() }.foregroundColor(accent)
                }
            }
        }
        .preferredColorScheme(.dark)
        .onAppear { runner.restoreIfAvailable(packages: packages) }
    }

    private func iconName(for state: DeveloperBatchTestRow.State) -> String {
        switch state {
        case .queued: return "clock"
        case .testing: return "gearshape.2"
        case .readyForManualCheck: return "checkmark.circle.fill"
        case .partial, .snapshotComplete: return "waveform.path"
        case .blocked: return "exclamationmark.triangle.fill"
        case .cancelled: return "stop.circle"
        }
    }

    private func color(for state: DeveloperBatchTestRow.State, accent: Color) -> Color {
        switch state {
        case .readyForManualCheck: return .green
        case .blocked: return .red
        case .testing: return .orange
        case .queued, .partial, .snapshotComplete, .cancelled: return accent
        }
    }
}

#endif
