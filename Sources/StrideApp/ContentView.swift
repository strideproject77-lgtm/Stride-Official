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
    @State private var searchText = ""

    private let canvas = Color.black
    private let panel = Color(red: 0.075, green: 0.075, blue: 0.082)
    private let raised = Color(red: 0.115, green: 0.115, blue: 0.125)
    private let accent = Color(red: 0.72, green: 0.96, blue: 0.30)
    private let muted = Color(red: 0.55, green: 0.56, blue: 0.59)
    private let line = Color.white.opacity(0.075)

    private var filteredPackages: [ImportedAPK] {
        guard !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return library.packages
        }
        return library.packages.filter {
            $0.displayName.localizedCaseInsensitiveContains(searchText) ||
            $0.inspection.summary.localizedCaseInsensitiveContains(searchText)
        }
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Group {
                switch selectedTab {
                case 1: libraryView
                case 2: runtimeView
                case 3: settingsView
                default: home
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            tabBar
        }
        .background(canvas.ignoresSafeArea())
        .preferredColorScheme(.dark)
        .fileImporter(
            isPresented: $showingImporter,
            // Accept generic Files-provider items; validate the .apk extension after selection.
            allowedContentTypes: [.item],
            allowsMultipleSelection: false,
            onCompletion: handleImport
        )
        .sheet(item: $selectedPackage) { item in
            APKDetailSheet(item: item, accent: accent, canvas: canvas, panel: panel, raised: raised, muted: muted)
        }
        .alert(importAlertTitle, isPresented: Binding(
            get: { importError != nil },
            set: { if !$0 { importError = nil } }
        )) {
            Button("OK", role: .cancel) { importError = nil }
        } message: {
            Text(importError ?? "Please try another APK file.")
        }
    }

    private var header: some View {
        HStack(spacing: 11) {
            ZStack {
                RoundedRectangle(cornerRadius: 13)
                    .fill(accent)
                    .frame(width: 38, height: 38)
                Image(systemName: "bolt.fill")
                    .font(.system(size: 19, weight: .black))
                    .foregroundStyle(.black)
            }
            VStack(alignment: .leading, spacing: 1) {
                Text("STRIDE")
                    .font(.system(size: 18, weight: .black, design: .rounded))
                    .kerning(2.6)
                Text("ANDROID APP PLATFORM")
                    .font(.system(size: 8, weight: .bold))
                    .kerning(1.5)
                    .foregroundStyle(muted)
            }
            Spacer()
            if selectedTab == 1 {
                Button { showingImporter = true } label: {
                    Group {
                        if isImporting {
                            ProgressView().tint(.black)
                        } else {
                            Image(systemName: "plus")
                                .font(.system(size: 15, weight: .bold))
                        }
                    }
                    .foregroundStyle(.black)
                    .frame(width: 38, height: 38)
                    .background(accent, in: Circle())
                }
                .buttonStyle(.plain)
                .disabled(isImporting)
                .accessibilityLabel("Import APK")
            } else {
                HStack(spacing: 6) {
                    Circle().fill(accent).frame(width: 6, height: 6)
                    Text("PREVIEW")
                        .font(.system(size: 8, weight: .heavy))
                        .kerning(1)
                        .foregroundStyle(muted)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 8)
                .background(panel, in: Capsule())
                .overlay(Capsule().stroke(line, lineWidth: 1))
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 9)
        .padding(.bottom, 13)
    }

    private var home: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 23) {
                hero
                HStack(spacing: 10) {
                    statCard(title: "APPS ADDED", value: "\(library.packages.count)", symbol: "square.stack.3d.up.fill")
                    statCard(title: "DEX FILES", value: "\(library.packages.reduce(0) { $0 + $1.inspection.dexFileCount })", symbol: "curlybraces")
                }
                sectionHeading("RECENTLY ADDED", action: library.packages.isEmpty ? nil : "SEE ALL") {
                    selectedTab = 1
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
            .padding(.top, 7)
            .padding(.bottom, 25)
        }
    }

    private var hero: some View {
        ZStack(alignment: .topTrailing) {
            RoundedRectangle(cornerRadius: 27)
                .fill(Color(red: 0.09, green: 0.10, blue: 0.085))
            Circle()
                .fill(accent.opacity(0.16))
                .frame(width: 190, height: 190)
                .blur(radius: 45)
                .offset(x: 62, y: -65)
            VStack(alignment: .leading, spacing: 17) {
                HStack(spacing: 7) {
                    Image(systemName: "sparkle")
                        .font(.system(size: 10, weight: .bold))
                    Text("YOUR ANDROID LIBRARY")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.6)
                }
                .foregroundStyle(accent)
                Text("Your apps.\nOne new home.")
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .kerning(-1.1)
                    .lineSpacing(-1)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Bring your APK collection into Stride and inspect every package in one place.")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(Color.white.opacity(0.62))
                    .lineSpacing(3)
                    .fixedSize(horizontal: false, vertical: true)
                Button { showingImporter = true } label: {
                    HStack(spacing: 9) {
                        Image(systemName: "plus")
                            .font(.system(size: 13, weight: .black))
                        Text("Add your first APK")
                            .font(.system(size: 13, weight: .bold))
                        Spacer()
                        Image(systemName: "arrow.up.right")
                            .font(.system(size: 11, weight: .bold))
                    }
                    .foregroundStyle(.black)
                    .padding(.horizontal, 16)
                    .frame(height: 48)
                    .background(accent, in: RoundedRectangle(cornerRadius: 14))
                }
                .buttonStyle(.plain)
                .padding(.top, 1)
            }
            .padding(21)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay(RoundedRectangle(cornerRadius: 27).stroke(Color.white.opacity(0.08), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 27))
    }

    private func statCard(title: String, value: String, symbol: String) -> some View {
        HStack(spacing: 11) {
            Image(systemName: symbol)
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(accent)
                .frame(width: 36, height: 36)
                .background(accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 3) {
                Text(value)
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text(title)
                    .font(.system(size: 8, weight: .heavy))
                    .kerning(0.9)
                    .foregroundStyle(muted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
        }
        .padding(12)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 17))
        .overlay(RoundedRectangle(cornerRadius: 17).stroke(line, lineWidth: 1))
    }

    private func sectionHeading(_ title: String, action: String?, onAction: @escaping () -> Void) -> some View {
        HStack {
            Text(title)
                .font(.system(size: 10, weight: .heavy))
                .kerning(1.5)
                .foregroundStyle(muted)
            Spacer()
            if let action {
                Button(action, action: onAction)
                    .font(.system(size: 9, weight: .heavy))
                    .kerning(0.8)
                    .foregroundStyle(accent)
                    .buttonStyle(.plain)
            }
        }
    }

    private var libraryView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 17) {
                HStack(alignment: .center) {
                    VStack(alignment: .leading, spacing: 5) {
                        Text("Library")
                            .font(.system(size: 29, weight: .bold, design: .rounded))
                        Text("\(library.packages.count) package\(library.packages.count == 1 ? "" : "s") in your collection")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(muted)
                    }
                    Spacer()
                    Button { showingImporter = true } label: {
                        Image(systemName: "plus")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(.black)
                            .frame(width: 42, height: 42)
                            .background(accent, in: RoundedRectangle(cornerRadius: 14))
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Import APK")
                }
                HStack(spacing: 10) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(muted)
                    TextField("Search your apps…", text: $searchText)
                        .font(.system(size: 13, weight: .medium))
                        .tint(accent)
                        .autocorrectionDisabled()
                }
                .padding(.horizontal, 14)
                .frame(height: 46)
                .background(panel, in: RoundedRectangle(cornerRadius: 14))
                .overlay(RoundedRectangle(cornerRadius: 14).stroke(line, lineWidth: 1))

                if filteredPackages.isEmpty {
                    if library.packages.isEmpty {
                        emptyState
                    } else {
                        VStack(spacing: 9) {
                            Image(systemName: "magnifyingglass")
                                .font(.system(size: 24))
                                .foregroundStyle(muted)
                            Text("No matching apps")
                                .font(.system(size: 15, weight: .semibold))
                            Text("Try another app name.")
                                .font(.system(size: 12))
                                .foregroundStyle(muted)
                        }
                        .frame(maxWidth: .infinity)
                        .padding(.vertical, 45)
                    }
                } else {
                    VStack(spacing: 9) {
                        ForEach(filteredPackages) { item in
                            packageButton(item)
                        }
                    }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 9)
            .padding(.bottom, 25)
        }
    }

    private var runtimeView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("Runtime")
                        .font(.system(size: 29, weight: .bold, design: .rounded))
                    Text("Engine components and current integration")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(muted)
                }
                VStack(spacing: 9) {
                    capabilityRow("APK archive inspection", detail: "Package structure and metadata", state: "READY", symbol: "checkmark.circle.fill", color: accent)
                    capabilityRow("DEX metadata analysis", detail: "Rust parser with bounded DEX extraction", state: "CONNECTED", symbol: "curlybraces", color: Color.orange)
                    capabilityRow("ART execution engine", detail: "DEX loader and interpreter foundations", state: "INTEGRATED", symbol: "cpu", color: accent)
                    capabilityRow("JNI compatibility", detail: "Native registry and selected system bridges", state: "INTEGRATED", symbol: "point.3.connected.trianglepath.dotted", color: Color.orange)
                }
                platformNote
                Text("Integration status does not mean every Android app is compatible. Real device execution is the compatibility test.")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(muted)
                    .padding(.horizontal, 3)
            }
            .padding(.horizontal, 18)
            .padding(.top, 9)
            .padding(.bottom, 25)
        }
    }

    private var settingsView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 19) {
                VStack(alignment: .leading, spacing: 5) {
                    Text("Settings")
                        .font(.system(size: 29, weight: .bold, design: .rounded))
                    Text("Stride preferences and information")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(muted)
                }
                VStack(spacing: 0) {
                    settingsRow("Appearance", detail: "Pure black interface", symbol: "moon.fill", trailing: "DARK")
                    Divider().overlay(line)
                    settingsRow("App library", detail: "\(library.packages.count) imported packages", symbol: "square.grid.2x2", trailing: nil)
                    Divider().overlay(line)
                    settingsRow("Runtime", detail: "Experimental compatibility layer", symbol: "cpu", trailing: nil)
                }
                .padding(.horizontal, 14)
                .background(panel, in: RoundedRectangle(cornerRadius: 18))
                .overlay(RoundedRectangle(cornerRadius: 18).stroke(line, lineWidth: 1))

                VStack(alignment: .leading, spacing: 8) {
                    Text("ABOUT STRIDE")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.5)
                        .foregroundStyle(muted)
                    Text("Built to bring Android package support to iPhone through a native compatibility layer.")
                        .font(.system(size: 13, weight: .medium))
                        .foregroundStyle(Color.white.opacity(0.78))
                        .lineSpacing(3)
                    Text("Stride · Experimental build")
                        .font(.system(size: 10, weight: .semibold))
                        .foregroundStyle(muted)
                        .padding(.top, 4)
                }
                .padding(17)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(panel, in: RoundedRectangle(cornerRadius: 18))
                .overlay(RoundedRectangle(cornerRadius: 18).stroke(line, lineWidth: 1))
            }
            .padding(.horizontal, 18)
            .padding(.top, 9)
            .padding(.bottom, 25)
        }
    }

    private func settingsRow(_ title: String, detail: String, symbol: String, trailing: String?) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(accent)
                .frame(width: 34, height: 34)
                .background(accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 10))
            VStack(alignment: .leading, spacing: 3) {
                Text(title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
            }
            Spacer()
            if let trailing {
                Text(trailing)
                    .font(.system(size: 8, weight: .heavy))
                    .kerning(0.7)
                    .foregroundStyle(accent)
                    .padding(.horizontal, 9)
                    .padding(.vertical, 6)
                    .background(accent.opacity(0.10), in: Capsule())
            }
        }
        .padding(.vertical, 13)
    }

    private var platformNote: some View {
        HStack(alignment: .top, spacing: 11) {
            Image(systemName: "info.circle.fill")
                .font(.system(size: 15))
                .foregroundStyle(accent)
            Text("Stride is experimental. Package inspection is available; Android app compatibility must be verified through actual runtime tests.")
                .font(.system(size: 11, weight: .medium))
                .foregroundStyle(muted)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(15)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))
    }

    private func capabilityRow(
        _ title: String, detail: String, state: String, symbol: String, color: Color
    ) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(color)
                .frame(width: 38, height: 38)
                .background(color.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 12, weight: .semibold))
                Text(detail).font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 4)
            Text(state)
                .font(.system(size: 7, weight: .heavy))
                .kerning(0.4)
                .foregroundStyle(color)
                .padding(.horizontal, 7)
                .padding(.vertical, 6)
                .background(color.opacity(0.10), in: Capsule())
        }
        .padding(13)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 16))
        .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))
    }

    private var emptyState: some View {
        VStack(spacing: 13) {
            ZStack {
                RoundedRectangle(cornerRadius: 20)
                    .fill(accent.opacity(0.09))
                    .frame(width: 66, height: 66)
                Image(systemName: "square.stack.3d.up")
                    .font(.system(size: 26, weight: .medium))
                    .foregroundStyle(accent)
            }
            Text("Your library starts here")
                .font(.system(size: 16, weight: .bold))
            Text("Add an APK to inspect its package structure, DEX files and native architectures.")
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(muted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: 275)
            Button { showingImporter = true } label: {
                Text("IMPORT APK")
                    .font(.system(size: 10, weight: .heavy))
                    .kerning(1)
                    .foregroundStyle(.black)
                    .padding(.horizontal, 19)
                    .frame(height: 40)
                    .background(accent, in: Capsule())
            }
            .buttonStyle(.plain)
            .padding(.top, 3)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 28)
        .padding(.horizontal, 17)
        .background(panel, in: RoundedRectangle(cornerRadius: 20))
        .overlay(RoundedRectangle(cornerRadius: 20).stroke(line, lineWidth: 1))
    }

    private func packageButton(_ item: ImportedAPK) -> some View {
        Button { selectedPackage = item } label: {
            APKRow(item: item, accent: accent, panel: panel, raised: raised, muted: muted, line: line)
        }
        .buttonStyle(.plain)
        .accessibilityHint("Opens package inspection details")
    }

    private var tabBar: some View {
        HStack(spacing: 4) {
            tab("house.fill", title: "Home", index: 0)
            tab("square.grid.2x2.fill", title: "Library", index: 1)
            tab("cpu", title: "Runtime", index: 2)
            tab("slider.horizontal.3", title: "Settings", index: 3)
        }
        .padding(.horizontal, 10)
        .padding(.top, 9)
        .padding(.bottom, 5)
        .background(canvas)
        .overlay(alignment: .top) {
            Rectangle().fill(line).frame(height: 1)
        }
    }

    private func tab(_ symbol: String, title: String, index: Int) -> some View {
        Button { selectedTab = index } label: {
            VStack(spacing: 5) {
                Image(systemName: symbol).font(.system(size: 15, weight: .semibold))
                Text(title).font(.system(size: 9, weight: .semibold))
            }
            .foregroundStyle(selectedTab == index ? accent : muted)
            .frame(maxWidth: .infinity)
            .frame(height: 45)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selectedTab == index ? .isSelected : [])
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
                    Image(systemName: "app.dashed")
                        .font(.system(size: size * 0.44, weight: .medium))
                        .foregroundStyle(accent)
                }
            }
        }
        .frame(width: size, height: size)
        .accessibilityHidden(true)
    }
}
