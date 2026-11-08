import SwiftUI
import UIKit
import UniformTypeIdentifiers

private extension UTType {
    static let androidPackage = UTType(filenameExtension: "apk") ?? .data
}

struct ContentView: View {
    @EnvironmentObject private var library: APKLibraryStore
    @StateObject private var diagnostics = StrideDiagnostics.shared
    @State private var showingImporter = false
    @State private var selectedTab = 0
    @State private var importError: String?

    private let background = Color(red: 0.055, green: 0.047, blue: 0.043)
    private let panel = Color(red: 0.105, green: 0.087, blue: 0.075)
    private let accent = Color(red: 0.78, green: 0.55, blue: 0.34)

    var body: some View {
        ZStack {
            background.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                Group {
                    if selectedTab == 0 {
                        home
                    } else if selectedTab == 1 {
                        libraryView
                    } else {
                        runtimeView
                    }
                }
                Spacer(minLength: 12)
                tabBar
            }
        }
        .fileImporter(
            isPresented: $showingImporter,
            allowedContentTypes: [.androidPackage],
            allowsMultipleSelection: true,
            onCompletion: handleImport
        )
        .alert("Import failed", isPresented: Binding(
            get: { importError != nil },
            set: { if !$0 { importError = nil } }
        )) {
            Button("OK", role: .cancel) { importError = nil }
        } message: {
            Text(importError ?? "Unknown error")
        }
    }

    private var header: some View {
        HStack {
            VStack(alignment: .leading, spacing: 3) {
                Text("STRIDE")
                    .font(.system(size: 25, weight: .black, design: .rounded))
                    .tracking(2.5)
                Text("NATIVE ANDROID COMPATIBILITY")
                    .font(.system(size: 9, weight: .semibold))
                    .tracking(1.5)
                    .foregroundStyle(.secondary)
            }
            Spacer()
            Image(systemName: "circle.hexagongrid.fill")
                .font(.system(size: 25, weight: .light))
                .foregroundStyle(accent)
        }
        .padding(.horizontal, 22)
        .padding(.top, 18)
        .padding(.bottom, 22)
    }

    private var home: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 10) {
                    Text("WELCOME TO STRIDE")
                        .font(.caption.weight(.bold))
                        .tracking(1.4)
                        .foregroundStyle(accent)
                    Text("Your apps.\nA native runtime.")
                        .font(.system(size: 34, weight: .bold, design: .rounded))
                    Text("Import Android packages and inspect their contents. Native execution support is under active development.")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .fixedSize(horizontal: false, vertical: true)
                    Button {
                        showingImporter = true
                    } label: {
                        Label("Import APK", systemImage: "plus")
                            .font(.headline)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 15)
                            .background(accent, in: RoundedRectangle(cornerRadius: 14))
                            .foregroundStyle(.black)
                    }
                    .padding(.top, 6)
                }
                .padding(20)
                .background(panel, in: RoundedRectangle(cornerRadius: 22))

                HStack {
                    Text("RECENTLY IMPORTED")
                        .font(.caption.weight(.bold))
                        .tracking(1.2)
                    Spacer()
                    Text("\(library.packages.count)")
                        .font(.caption.monospacedDigit())
                        .foregroundStyle(.secondary)
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    ForEach(library.packages.prefix(3)) { item in
                        APKRow(item: item, accent: accent)
                    }
                }
            }
            .padding(.horizontal, 18)
        }
    }

    private var libraryView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    Text("YOUR LIBRARY")
                        .font(.caption.weight(.bold))
                        .tracking(1.3)
                    Spacer()
                    Button { showingImporter = true } label: {
                        Image(systemName: "plus.circle.fill")
                            .font(.title2)
                            .foregroundStyle(accent)
                    }
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    ForEach(library.packages) { item in
                        APKRow(item: item, accent: accent)
                    }
                }
            }
            .padding(.horizontal, 18)
        }
    }

    private var runtimeView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Text("RUNTIME STATUS")
                    .font(.caption.weight(.bold))
                    .tracking(1.3)
                statusCard("APK archive inspection", detail: "Available", symbol: "checkmark.circle.fill", good: true)
                statusCard("Android manifest decoding", detail: "Not implemented", symbol: "circle.dashed", good: false)
                statusCard("DEX / ART execution", detail: "Not implemented", symbol: "circle.dashed", good: false)
                statusCard("Bionic / JNI compatibility", detail: "Not implemented", symbol: "circle.dashed", good: false)
                statusCard("Native ARM translation", detail: "Not implemented", symbol: "circle.dashed", good: false)
                VStack(alignment: .leading, spacing: 10) {
                    HStack {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("DIAGNOSTIC LOG")
                                .font(.caption.weight(.bold))
                                .tracking(1.2)
                            Text("\(diagnostics.issueCount) errors collected · \(diagnostics.entries.count) recent events")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Button {
                            UIPasteboard.general.string = diagnostics.entries.map(\.line).joined(separator: "\\n")
                        } label: {
                            Label("Copy", systemImage: "doc.on.doc")
                                .font(.caption.weight(.semibold))
                                .foregroundStyle(accent)
                        }
                    }
                    if diagnostics.entries.isEmpty {
                        Text("No diagnostic events yet. Import an APK to begin a report.")
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    } else {
                        ForEach(diagnostics.entries.suffix(80)) { entry in
                            VStack(alignment: .leading, spacing: 3) {
                                HStack(spacing: 6) {
                                    Text(entry.severity.rawValue)
                                        .font(.system(size: 9, weight: .bold, design: .monospaced))
                                        .foregroundStyle(entry.severity == .error || entry.severity == .fatal ? .red : (entry.severity == .warning ? .orange : accent))
                                    Text(entry.stage)
                                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                    Spacer()
                                    Text(entry.code)
                                        .font(.system(size: 9, design: .monospaced))
                                        .foregroundStyle(.secondary)
                                }
                                Text(entry.message)
                                    .font(.caption)
                                    .textSelection(.enabled)
                                if let recovery = entry.recovery {
                                    Text("Recovery: " + recovery)
                                        .font(.system(size: 10))
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .padding(9)
                            .frame(maxWidth: .infinity, alignment: .leading)
                            .background(panel, in: RoundedRectangle(cornerRadius: 10))
                        }
                    }
                    Text("Full log is saved as stride-diagnostics.log in the app's Documents folder. Diagnostics continue across independent checks; unsafe execution is never skipped.")
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                }
                .padding(14)
                .background(panel.opacity(0.7), in: RoundedRectangle(cornerRadius: 16))
                .padding(.top, 4)
            }
            .padding(.horizontal, 18)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 10) {
            Image(systemName: "square.and.arrow.down")
                .font(.system(size: 30))
                .foregroundStyle(accent)
            Text("No APKs yet")
                .font(.headline)
            Text("Import an APK to inspect its package structure.")
                .font(.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 35)
        .background(panel, in: RoundedRectangle(cornerRadius: 18))
    }

    private var tabBar: some View {
        HStack(spacing: 0) {
            tab("house.fill", title: "Home", index: 0)
            tab("square.grid.2x2.fill", title: "Library", index: 1)
            tab("cpu", title: "Runtime", index: 2)
        }
        .padding(.top, 12)
        .padding(.bottom, 8)
        .background(panel)
    }

    private func tab(_ symbol: String, title: String, index: Int) -> some View {
        Button { selectedTab = index } label: {
            VStack(spacing: 5) {
                Image(systemName: symbol).font(.system(size: 18))
                Text(title).font(.system(size: 10, weight: .semibold))
            }
            .foregroundStyle(selectedTab == index ? accent : .secondary)
            .frame(maxWidth: .infinity)
        }
    }

    private func statusCard(_ title: String, detail: String, symbol: String, good: Bool) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.title3)
                .foregroundStyle(good ? .green : .secondary)
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.subheadline.weight(.semibold))
                Text(detail).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
        }
        .padding(15)
        .background(panel, in: RoundedRectangle(cornerRadius: 14))
    }

    private func handleImport(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            do {
                for url in urls { try library.importPackage(from: url) }
            } catch {
                importError = error.localizedDescription
            }
        case .failure(let error):
            importError = error.localizedDescription
        }
    }
}

private struct APKRow: View {
    let item: ImportedAPK
    let accent: Color

    var body: some View {
        HStack(spacing: 13) {
            RoundedRectangle(cornerRadius: 12)
                .fill(accent.opacity(0.16))
                .frame(width: 48, height: 48)
                .overlay {
                    Image(systemName: "app.dashed")
                        .font(.title2)
                        .foregroundStyle(accent)
                }
            VStack(alignment: .leading, spacing: 4) {
                Text(item.displayName)
                    .font(.subheadline.weight(.semibold))
                    .lineLimit(1)
                Text(item.inspection.summary)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer()
            Image(systemName: item.inspection.isValidPackage ? "checkmark.circle.fill" : "exclamationmark.circle")
                .foregroundStyle(item.inspection.isValidPackage ? .green : .orange)
        }
        .padding(13)
        .background(Color(red: 0.105, green: 0.087, blue: 0.075), in: RoundedRectangle(cornerRadius: 15))
    }
}
