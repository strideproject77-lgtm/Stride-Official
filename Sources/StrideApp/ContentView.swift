import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct ContentView: View {
    @EnvironmentObject private var library: APKLibraryStore
    @State private var showingImporter = false
    @State private var selectedPackage: ImportedAPK?
    @State private var selectedTab = 0
    @State private var importError: String?
    @State private var importAlertTitle = "Import failed"
    @State private var isImporting = false

    // Main-branch layout, recolored for a true black interface.
    private let background = Color.black
    private let panel = Color(red: 0.075, green: 0.075, blue: 0.082)
    private let raised = Color(red: 0.115, green: 0.115, blue: 0.125)
    private let accent = Color(red: 0.72, green: 0.96, blue: 0.30)
    private let muted = Color(red: 0.58, green: 0.59, blue: 0.62)
    private let line = Color.white.opacity(0.075)

    var body: some View {
        ZStack {
            background.ignoresSafeArea()
            VStack(spacing: 0) {
                header
                    .sheet(isPresented: $showingImporter) {
                        APKDocumentPicker { result in handleImport(result) }
                    }
                Group {
                    if selectedTab == 0 {
                        home
                    } else if selectedTab == 1 {
                        libraryView
                    } else {
                        runtimeView
                    }
                }
                .sheet(item: $selectedPackage) { item in
                    APKDetailSheet(item: item, accent: accent, canvas: background, panel: panel, raised: raised, muted: muted)
                }
                Spacer(minLength: 12)
                tabBar
            }
        }
        .alert(importAlertTitle, isPresented: Binding(
            get: { importError != nil },
            set: { if !$0 { importError = nil } }
        )) {
            Button("OK", role: .cancel) { importError = nil }
        } message: {
            Text(importError ?? "Please try another APK file.")
        }
        .preferredColorScheme(.dark)
    }

    private var header: some View {
        HStack(spacing: 11) {
            ZStack {
                RoundedRectangle(cornerRadius: 12)
                    .fill(accent)
                    .frame(width: 38, height: 38)
                Image(systemName: "bolt.fill")
                    .font(.system(size: 19, weight: .black))
                    .foregroundStyle(.black)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text("STRIDE")
                    .font(.system(size: 21, weight: .black, design: .rounded))
                    .tracking(2.4)
                Text("NATIVE ANDROID COMPATIBILITY · 0.1")
                    .font(.system(size: 8, weight: .semibold))
                    .tracking(0.9)
                    .foregroundStyle(muted)
            }
            Spacer()
            if isImporting {
                ProgressView().tint(accent)
                    .frame(width: 34, height: 34)
            } else {
                Image(systemName: "circle.hexagongrid.fill")
                    .font(.system(size: 23, weight: .light))
                    .foregroundStyle(accent)
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 17)
    }

    private var home: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 11) {
                    Text("WELCOME TO STRIDE")
                        .font(.system(size: 10, weight: .heavy))
                        .tracking(1.5)
                        .foregroundStyle(accent)
                    Text("Your apps.\nA native runtime.")
                        .font(.system(size: 33, weight: .bold, design: .rounded))
                        .tracking(-0.7)
                        .fixedSize(horizontal: false, vertical: true)
                    Text("Bring your Android packages into one library and inspect their contents on iPhone.")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(muted)
                        .lineSpacing(3)
                        .fixedSize(horizontal: false, vertical: true)
                    Button {
                        showingImporter = true
                    } label: {
                        HStack(spacing: 9) {
                            Image(systemName: "plus").font(.system(size: 13, weight: .black))
                            Text(isImporting ? "Importing APK…" : "Import APK")
                                .font(.system(size: 13, weight: .bold))
                            Spacer()
                            Image(systemName: "arrow.up.right").font(.system(size: 11, weight: .bold))
                        }
                        .foregroundStyle(.black)
                        .padding(.horizontal, 16)
                        .frame(height: 49)
                        .background(accent, in: RoundedRectangle(cornerRadius: 14))
                    }
                    .buttonStyle(.plain)
                    .disabled(isImporting)
                    .padding(.top, 4)
                }
                .padding(20)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(panel, in: RoundedRectangle(cornerRadius: 22))
                .overlay(RoundedRectangle(cornerRadius: 22).stroke(line, lineWidth: 1))

                HStack {
                    Text("RECENTLY IMPORTED")
                        .font(.system(size: 10, weight: .heavy))
                        .tracking(1.3)
                        .foregroundStyle(muted)
                    Spacer()
                    Text("\(library.packages.count)")
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(muted)
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    VStack(spacing: 9) {
                        ForEach(library.packages.prefix(4)) { item in
                            packageButton(item)
                        }
                    }
                }
                platformNote
            }
            .padding(.horizontal, 18)
            .padding(.top, 5)
            .padding(.bottom, 22)
        }
    }

    private var libraryView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 14) {
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("YOUR LIBRARY")
                            .font(.system(size: 11, weight: .heavy))
                            .tracking(1.4)
                        Text("\(library.packages.count) APK package\(library.packages.count == 1 ? "" : "s")")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(muted)
                    }
                    Spacer()
                    Button { showingImporter = true } label: {
                        Image(systemName: "plus")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(.black)
                            .frame(width: 42, height: 42)
                            .background(accent, in: RoundedRectangle(cornerRadius: 13))
                    }
                    .buttonStyle(.plain)
                    .disabled(isImporting)
                    .accessibilityLabel("Import APK")
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    VStack(spacing: 9) {
                        ForEach(library.packages) { item in
                            packageButton(item)
                        }
                    }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 22)
        }
    }

    private var runtimeView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 14) {
                Text("RUNTIME STATUS")
                    .font(.system(size: 11, weight: .heavy))
                    .tracking(1.4)
                    .foregroundStyle(muted)
                statusCard("APK archive inspection", detail: "Available", symbol: "checkmark.circle.fill", good: true)
                statusCard("DEX parsing and method loading", detail: "Integrated foundations", symbol: "curlybraces", good: true)
                statusCard("ART / Android framework", detail: "Partial implementation · not full compatibility", symbol: "cpu", good: false)
                statusCard("Native libraries and graphics", detail: "Work in progress", symbol: "square.stack.3d.up", good: false)
                platformNote
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 22)
        }
    }

    private var emptyState: some View {
        VStack(spacing: 10) {
            Image(systemName: "square.stack.3d.up")
                .font(.system(size: 29, weight: .medium))
                .foregroundStyle(accent)
            Text("No APKs yet")
                .font(.system(size: 15, weight: .bold))
            Text("Import an APK to inspect its package structure.")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(muted)
                .multilineTextAlignment(.center)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 32)
        .padding(.horizontal, 16)
        .background(panel, in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).stroke(line, lineWidth: 1))
    }

    private var platformNote: some View {
        HStack(alignment: .top, spacing: 10) {
            Image(systemName: "info.circle.fill")
                .font(.system(size: 14))
                .foregroundStyle(accent)
            Text("Stride is experimental. Package inspection is available; Android app compatibility must be verified through real runtime tests.")
                .font(.system(size: 10, weight: .medium))
                .foregroundStyle(muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 15))
        .overlay(RoundedRectangle(cornerRadius: 15).stroke(line, lineWidth: 1))
    }

    private var tabBar: some View {
        HStack(spacing: 0) {
            tab("house.fill", title: "Home", index: 0)
            tab("square.grid.2x2.fill", title: "Library", index: 1)
            tab("cpu", title: "Runtime", index: 2)
        }
        .padding(.top, 10)
        .padding(.bottom, 6)
        .background(panel)
        .overlay(alignment: .top) { Rectangle().fill(line).frame(height: 1) }
    }

    private func tab(_ symbol: String, title: String, index: Int) -> some View {
        Button { selectedTab = index } label: {
            VStack(spacing: 5) {
                Image(systemName: symbol).font(.system(size: 17, weight: .semibold))
                Text(title).font(.system(size: 10, weight: .semibold))
            }
            .foregroundStyle(selectedTab == index ? accent : muted)
            .frame(maxWidth: .infinity)
            .frame(height: 43)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selectedTab == index ? .isSelected : [])
    }

    private func statusCard(_ title: String, detail: String, symbol: String, good: Bool) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(good ? accent : muted)
                .frame(width: 38, height: 38)
                .background((good ? accent : muted).opacity(0.10), in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.system(size: 12, weight: .semibold))
                Text(detail).font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
            }
            Spacer(minLength: 0)
        }
        .padding(13)
        .background(panel, in: RoundedRectangle(cornerRadius: 15))
        .overlay(RoundedRectangle(cornerRadius: 15).stroke(line, lineWidth: 1))
    }

    private func packageButton(_ item: ImportedAPK) -> some View {
        Button { selectedPackage = item } label: {
            APKRow(item: item, accent: accent, panel: panel, raised: raised, muted: muted, line: line)
        }
        .buttonStyle(.plain)
        .accessibilityHint("Opens package inspection details")
    }

    private func handleImport(_ result: Result<[URL], Error>) {
        switch result {
        case .success(let urls):
            guard let url = urls.first else { return }
            isImporting = true
            Task { @MainActor in
                do {
                    try await library.importPackageAsync(from: url)
                    selectedTab = 1
                    importAlertTitle = "Import complete"
                    importError = "\(url.lastPathComponent) was added to your Stride library."
                } catch {
                    importAlertTitle = "Import failed"
                    importError = "\(url.lastPathComponent): \(error.localizedDescription)"
                }
                isImporting = false
            }
        case .failure(let error):
            importAlertTitle = "Import failed"
            importError = error.localizedDescription
        }
    }
}

private struct APKRow: View {
    let item: ImportedAPK
    let accent: Color
    let panel: Color
    let raised: Color
    let muted: Color
    let line: Color

    var body: some View {
        HStack(spacing: 13) {
            APKIconView(data: item.iconData, accent: accent, size: 49)
                .background(raised, in: RoundedRectangle(cornerRadius: 14))
                .clipShape(RoundedRectangle(cornerRadius: 14))
            VStack(alignment: .leading, spacing: 5) {
                Text(item.displayName)
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                    .foregroundStyle(.white)
                Text(item.inspection.summary)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(muted)
                    .lineLimit(1)
                HStack(spacing: 5) {
                    Circle().fill(accent).frame(width: 5, height: 5)
                    Text("PACKAGE INSPECTED")
                        .font(.system(size: 7, weight: .heavy))
                        .kerning(0.8)
                        .foregroundStyle(accent.opacity(0.9))
                }
            }
            Spacer(minLength: 4)
            Image(systemName: "arrow.up.right")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(muted)
                .frame(width: 29, height: 29)
                .background(raised, in: Circle())
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 17))
        .overlay(RoundedRectangle(cornerRadius: 17).stroke(line, lineWidth: 1))
        .contentShape(RoundedRectangle(cornerRadius: 17))
    }
}

private struct APKDetailSheet: View {
    @Environment(\.dismiss) private var dismiss
    let item: ImportedAPK
    @State private var isExecuting = false
    @State private var executionResult: APKExecutionResult?
    @State private var executionError: String?
    let accent: Color
    let canvas: Color
    let panel: Color
    let raised: Color
    let muted: Color

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("APP OVERVIEW")
                    .font(.system(size: 9, weight: .heavy))
                    .kerning(1.5)
                    .foregroundStyle(muted)
                Spacer()
                Button { dismiss() } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 11, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 33, height: 33)
                        .background(raised, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close package details")
            }
            .padding(.horizontal, 20)
            .padding(.top, 17)
            .padding(.bottom, 10)

            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18) {
                    HStack(spacing: 14) {
                        APKIconView(data: item.iconData, accent: accent, size: 66)
                            .background(raised, in: RoundedRectangle(cornerRadius: 18))
                            .clipShape(RoundedRectangle(cornerRadius: 18))
                        VStack(alignment: .leading, spacing: 5) {
                            Text(item.displayName)
                                .font(.system(size: 22, weight: .bold, design: .rounded))
                                .lineLimit(2)
                                .fixedSize(horizontal: false, vertical: true)
                            Text("Package information")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(muted)
                        }
                        Spacer(minLength: 0)
                    }
                    VStack(spacing: 0) {
                        detailRow("Archive", value: item.inspection.isValidPackage ? "Valid APK" : "Invalid", symbol: "archivebox")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Manifest", value: item.inspection.hasAndroidManifest ? "Found" : "Missing", symbol: "doc.text")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("DEX files", value: item.inspection.hasDEX ? "\(item.inspection.analyzedDexFileCount)/\(item.inspection.dexFileCount) analyzed" : "None found", symbol: "curlybraces")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("DEX classes", value: "\(item.inspection.dexClassCount)", symbol: "square.stack.3d.up")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("DEX methods", value: "\(item.inspection.dexMethodCount)", symbol: "function")
                        if item.inspection.skippedDexFileCount > 0 {
                            Divider().overlay(Color.white.opacity(0.05))
                            detailRow("DEX files skipped", value: "\(item.inspection.skippedDexFileCount)", symbol: "exclamationmark.triangle")
                        }
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Resource table", value: item.inspection.hasResourceTable ? "Found" : "Not found", symbol: "square.grid.3x3")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Archive entries", value: "\(item.inspection.archiveEntryCount)", symbol: "list.number")
                    }
                    .padding(.horizontal, 15)
                    .background(panel, in: RoundedRectangle(cornerRadius: 18))

                    VStack(alignment: .leading, spacing: 11) {
                        Text("NATIVE ARCHITECTURES")
                            .font(.system(size: 9, weight: .heavy))
                            .kerning(1.2)
                            .foregroundStyle(muted)
                        if item.inspection.nativeABIs.isEmpty {
                            Text("No native libraries detected")
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(muted)
                        } else {
                            FlowABIList(values: item.inspection.nativeABIs, accent: accent, raised: raised)
                        }
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(panel, in: RoundedRectangle(cornerRadius: 18))

                    Button {
                        Task {
                            isExecuting = true
                            executionError = nil
                            do {
                                let backend = StrideArtExecutionBackend()
                                let res = try await backend.launch(packageAt: item.fileURL)
                                executionResult = res
                            } catch {
                                executionError = error.localizedDescription
                            }
                            isExecuting = false
                        }
                    } label: {
                        HStack(spacing: 9) {
                            if isExecuting {
                                ProgressView().tint(.black)
                                Text("Starting runtime…")
                            } else {
                                Image(systemName: "play.fill")
                                Text("Launch in ART Runtime")
                            }
                        }
                        .font(.system(size: 13, weight: .bold))
                        .foregroundStyle(.black)
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 15)
                        .background(accent, in: RoundedRectangle(cornerRadius: 14))
                    }
                    .buttonStyle(.plain)
                    .disabled(isExecuting)

                    if let res = executionResult {
                        VStack(alignment: .leading, spacing: 7) {
                            HStack {
                                Image(systemName: "checkmark.circle.fill").foregroundStyle(accent)
                                Text("Runtime report").font(.system(size: 13, weight: .bold))
                            }
                            Text("Status: \(res.statusCode == 0 ? "Running" : "Finished") · Lifecycle: \(res.lifecycleState)")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(muted)
                            Text("Classes: \(res.totalClassesLoaded) · Instructions: \(res.instructionsExecuted) · Missing APIs: \(res.missingApisCount)")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(muted)
                        }
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(accent.opacity(0.09), in: RoundedRectangle(cornerRadius: 14))
                    } else if let err = executionError {
                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Image(systemName: "exclamationmark.triangle.fill").foregroundStyle(.red)
                                Text("Runtime launch error").font(.system(size: 13, weight: .bold))
                            }
                            Text(err).font(.system(size: 11, weight: .medium)).foregroundStyle(muted)
                        }
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.red.opacity(0.09), in: RoundedRectangle(cornerRadius: 14))
                    }
                }
                .padding(.horizontal, 20)
                .padding(.top, 12)
                .padding(.bottom, 30)
            }
        }
        .background(canvas.ignoresSafeArea())
        .preferredColorScheme(.dark)
    }

    private func detailRow(_ title: String, value: String, symbol: String) -> some View {
        HStack(spacing: 11) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(accent)
                .frame(width: 22)
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(muted)
            Spacer()
            Text(value)
                .font(.system(size: 12, weight: .semibold))
                .multilineTextAlignment(.trailing)
        }
        .padding(.vertical, 13)
    }
}

private struct FlowABIList: View {
    let values: [String]
    let accent: Color
    let raised: Color

    var body: some View {
        LazyVGrid(columns: [GridItem(.adaptive(minimum: 85), alignment: .leading)], alignment: .leading, spacing: 8) {
            ForEach(values, id: \.self) { value in
                Text(value)
                    .font(.system(size: 10, weight: .semibold, design: .monospaced))
                    .foregroundStyle(accent)
                    .padding(.horizontal, 11)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(raised, in: Capsule())
            }
        }
    }
}

private struct APKIconView: View {
    let data: Data?
    let accent: Color
    let size: CGFloat

    var body: some View {
        Group {
            if let data, let image = UIImage(data: data) {
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFit()
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.25)
                        .fill(LinearGradient(
                            colors: [accent.opacity(0.18), accent.opacity(0.05)],
                            startPoint: .topLeading,
                            endPoint: .bottomTrailing
                        ))
                    Image(systemName: "square.stack.3d.up.fill")
                        .font(.system(size: size * 0.40, weight: .medium))
                        .foregroundStyle(accent)
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}


/// Uses the iOS document provider in copy mode so Stride receives a sandbox-local file.
private struct APKDocumentPicker: UIViewControllerRepresentable {
    let onPick: (Result<[URL], Error>) -> Void

    func makeCoordinator() -> Coordinator { Coordinator(onPick: onPick) }

    func makeUIViewController(context: Context) -> UIDocumentPickerViewController {
        let picker = UIDocumentPickerViewController(forOpeningContentTypes: [.item], asCopy: true)
        picker.allowsMultipleSelection = false
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: UIDocumentPickerViewController, context: Context) {}

    final class Coordinator: NSObject, UIDocumentPickerDelegate {
        private let onPick: (Result<[URL], Error>) -> Void

        init(onPick: @escaping (Result<[URL], Error>) -> Void) { self.onPick = onPick }

        func documentPicker(_ controller: UIDocumentPickerViewController, didPickDocumentsAt urls: [URL]) {
            onPick(.success(urls))
        }

        func documentPickerWasCancelled(_ controller: UIDocumentPickerViewController) {
            onPick(.success([]))
        }
    }
}
