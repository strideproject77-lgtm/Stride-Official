import SwiftUI
import UniformTypeIdentifiers

private extension UTType {
    static let androidPackage = UTType(filenameExtension: "apk") ?? .data
}

struct ContentView: View {
    @EnvironmentObject private var library: APKLibraryStore
    @State private var showingImporter = false
    @State private var selectedPackage: ImportedAPK?
    @State private var selectedTab = 0
    @State private var importError: String?

    private let canvas = Color(red: 0.035, green: 0.047, blue: 0.060)
    private let panel = Color(red: 0.075, green: 0.094, blue: 0.112)
    private let raised = Color(red: 0.105, green: 0.129, blue: 0.149)
    private let accent = Color(red: 0.40, green: 0.91, blue: 0.76)
    private let muted = Color(red: 0.56, green: 0.63, blue: 0.68)

    var body: some View {
        VStack(spacing: 0) {
            header
            Group {
                switch selectedTab {
                case 1: libraryView
                case 2: runtimeView
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
            allowedContentTypes: [.androidPackage],
            allowsMultipleSelection: true,
            onCompletion: handleImport
        )
        .sheet(item: $selectedPackage) { item in
            APKDetailSheet(item: item, accent: accent, canvas: canvas, panel: panel, raised: raised, muted: muted)
        }
        .alert("Couldn't import APK", isPresented: Binding(
            get: { importError != nil },
            set: { if !$0 { importError = nil } }
        )) {
            Button("OK", role: .cancel) { importError = nil }
        } message: {
            Text(importError ?? "Please try another APK file.")
        }
    }

    private var header: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 13)
                    .fill(accent.opacity(0.13))
                    .frame(width: 42, height: 42)
                Image(systemName: "circle.hexagongrid.fill")
                    .font(.system(size: 22, weight: .medium))
                    .foregroundStyle(accent)
            }
            VStack(alignment: .leading, spacing: 2) {
                Text("STRIDE")
                    .font(.system(size: 19, weight: .black, design: .rounded))
                    .tracking(2.4)
                Text("ANDROID COMPATIBILITY")
                    .font(.system(size: 8, weight: .bold))
                    .tracking(1.4)
                    .foregroundStyle(muted)
            }
            Spacer()
            if selectedTab == 1 {
                Button { showingImporter = true } label: {
                    Image(systemName: "plus")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(canvas)
                        .frame(width: 38, height: 38)
                        .background(accent, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Import APK")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 10)
        .padding(.bottom, 14)
    }

    private var home: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 23) {
                hero
                HStack(spacing: 12) {
                    statCard(title: "IN YOUR LIBRARY", value: "\(library.packages.count)", symbol: "square.stack.3d.up.fill")
                    statCard(title: "DEX FILES", value: "\(library.packages.reduce(0) { $0 + $1.inspection.dexFileCount })", symbol: "chevron.left.forwardslash.chevron.right")
                }
                HStack(alignment: .firstTextBaseline) {
                    Text("RECENT PACKAGES")
                        .font(.system(size: 11, weight: .bold))
                        .tracking(1.4)
                        .foregroundStyle(muted)
                    Spacer()
                    if !library.packages.isEmpty {
                        Button("VIEW ALL") { selectedTab = 1 }
                            .font(.system(size: 10, weight: .bold))
                            .tracking(1)
                            .foregroundStyle(accent)
                            .buttonStyle(.plain)
                    }
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    VStack(spacing: 10) {
                        ForEach(library.packages.prefix(3)) { item in
                            packageButton(item)
                        }
                    }
                }
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "info.circle.fill")
                        .foregroundStyle(accent)
                    Text("Stride currently inspects APK archives. Android app execution is still in development.")
                        .font(.footnote)
                        .foregroundStyle(muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(15)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(panel, in: RoundedRectangle(cornerRadius: 16))
            }
            .padding(.horizontal, 18)
            .padding(.top, 5)
            .padding(.bottom, 22)
        }
    }

    private var hero: some View {
        ZStack(alignment: .topTrailing) {
            RoundedRectangle(cornerRadius: 25)
                .fill(LinearGradient(
                    colors: [Color(red: 0.10, green: 0.22, blue: 0.23), panel],
                    startPoint: .topLeading,
                    endPoint: .bottomTrailing
                ))
            Circle()
                .fill(accent.opacity(0.12))
                .frame(width: 150, height: 150)
                .blur(radius: 3)
                .offset(x: 38, y: -52)
                .clipped()
            VStack(alignment: .leading, spacing: 14) {
                HStack(spacing: 7) {
                    Circle().fill(accent).frame(width: 6, height: 6)
                    Text("PACKAGE ANALYSIS")
                        .font(.system(size: 10, weight: .bold))
                        .tracking(1.6)
                        .foregroundStyle(accent)
                }
                Text("Know what’s\ninside every APK.")
                    .font(.system(size: 30, weight: .bold, design: .rounded))
                    .tracking(-0.7)
                    .fixedSize(horizontal: false, vertical: true)
                Text("Inspect package structure now. Build toward native Android compatibility, one verified layer at a time.")
                    .font(.subheadline)
                    .foregroundStyle(Color.white.opacity(0.70))
                    .lineSpacing(3)
                    .fixedSize(horizontal: false, vertical: true)
                Button { showingImporter = true } label: {
                    HStack(spacing: 9) {
                        Image(systemName: "plus")
                            .font(.system(size: 13, weight: .bold))
                        Text("Import APK")
                            .font(.system(size: 15, weight: .bold))
                        Spacer()
                        Image(systemName: "arrow.up.right")
                            .font(.system(size: 12, weight: .bold))
                    }
                    .foregroundStyle(canvas)
                    .padding(.horizontal, 17)
                    .frame(height: 50)
                    .background(accent, in: RoundedRectangle(cornerRadius: 14))
                }
                .buttonStyle(.plain)
                .padding(.top, 3)
            }
            .padding(21)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .overlay(RoundedRectangle(cornerRadius: 25).stroke(Color.white.opacity(0.06), lineWidth: 1))
        .clipShape(RoundedRectangle(cornerRadius: 25))
    }

    private func statCard(title: String, value: String, symbol: String) -> some View {
        HStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(accent)
                .frame(width: 38, height: 38)
                .background(accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 3) {
                Text(value)
                    .font(.system(size: 22, weight: .bold, design: .rounded))
                    .monospacedDigit()
                Text(title)
                    .font(.system(size: 8, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(muted)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
            }
            Spacer(minLength: 0)
        }
        .padding(13)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 17))
    }

    private var libraryView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("YOUR LIBRARY")
                        .font(.system(size: 23, weight: .bold, design: .rounded))
                    Text("\(library.packages.count) imported package\(library.packages.count == 1 ? "" : "s")")
                        .font(.subheadline)
                        .foregroundStyle(muted)
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    VStack(spacing: 10) {
                        ForEach(library.packages) { item in
                            packageButton(item)
                        }
                    }
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }

    private var runtimeView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 6) {
                    Text("RUNTIME STATUS")
                        .font(.system(size: 23, weight: .bold, design: .rounded))
                    Text("Implementation progress, not compatibility claims.")
                        .font(.subheadline)
                        .foregroundStyle(muted)
                }
                VStack(spacing: 10) {
                    capabilityRow("APK archive inspection", detail: "Available in the app", state: "READY", symbol: "checkmark.circle.fill", color: accent)
                    capabilityRow("DEX analysis", detail: "Parser in development; not connected to import yet", state: "BUILDING", symbol: "gearshape.2.fill", color: Color.orange)
                    capabilityRow("Android code execution", detail: "No APK launches in this build", state: "NOT READY", symbol: "circle.dashed", color: muted)
                    capabilityRow("Native libraries, JNI and Android APIs", detail: "Not implemented yet", state: "NOT READY", symbol: "circle.dashed", color: muted)
                }
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: "shield.lefthalf.filled")
                        .foregroundStyle(accent)
                    Text("An APK is marked runnable only after its code launches and passes real device checks. Importing or inspecting a package does not count as execution.")
                        .font(.footnote)
                        .foregroundStyle(muted)
                        .fixedSize(horizontal: false, vertical: true)
                }
                .padding(16)
                .frame(maxWidth: .infinity, alignment: .leading)
                .background(panel, in: RoundedRectangle(cornerRadius: 17))
            }
            .padding(.horizontal, 18)
            .padding(.top, 8)
            .padding(.bottom, 24)
        }
    }

    private func capabilityRow(
        _ title: String, detail: String, state: String, symbol: String, color: Color
    ) -> some View {
        HStack(spacing: 13) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(color)
                .frame(width: 40, height: 40)
                .background(color.opacity(0.10), in: RoundedRectangle(cornerRadius: 12))
            VStack(alignment: .leading, spacing: 4) {
                Text(title).font(.system(size: 13, weight: .semibold))
                Text(detail).font(.caption).foregroundStyle(muted).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 6)
            Text(state)
                .font(.system(size: 8, weight: .heavy))
                .tracking(0.5)
                .foregroundStyle(color)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background(color.opacity(0.10), in: Capsule())
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(panel, in: RoundedRectangle(cornerRadius: 17))
    }

    private var emptyState: some View {
        VStack(spacing: 13) {
            Image(systemName: "shippingbox")
                .font(.system(size: 27, weight: .light))
                .foregroundStyle(accent)
                .frame(width: 62, height: 62)
                .background(accent.opacity(0.10), in: RoundedRectangle(cornerRadius: 19))
            Text("Your library is ready")
                .font(.headline)
            Text("Import an APK to see its DEX files, native architectures and package structure.")
                .font(.subheadline)
                .foregroundStyle(muted)
                .multilineTextAlignment(.center)
                .fixedSize(horizontal: false, vertical: true)
                .frame(maxWidth: 290)
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 30)
        .padding(.horizontal, 18)
        .background(panel, in: RoundedRectangle(cornerRadius: 20))
        .overlay(RoundedRectangle(cornerRadius: 20).stroke(Color.white.opacity(0.05), lineWidth: 1))
    }

    private func packageButton(_ item: ImportedAPK) -> some View {
        Button { selectedPackage = item } label: {
            APKRow(item: item, accent: accent, panel: panel, raised: raised, muted: muted)
        }
        .buttonStyle(.plain)
        .accessibilityHint("Opens package inspection details")
    }

    private var tabBar: some View {
        HStack(spacing: 7) {
            tab("house.fill", title: "Home", index: 0)
            tab("square.grid.2x2.fill", title: "Library", index: 1)
            tab("cpu", title: "Runtime", index: 2)
        }
        .padding(.horizontal, 14)
        .padding(.top, 10)
        .padding(.bottom, 6)
        .background(canvas.opacity(0.98))
        .overlay(alignment: .top) {
            Rectangle().fill(Color.white.opacity(0.06)).frame(height: 1)
        }
    }

    private func tab(_ symbol: String, title: String, index: Int) -> some View {
        Button { selectedTab = index } label: {
            HStack(spacing: 7) {
                Image(systemName: symbol).font(.system(size: 13, weight: .semibold))
                Text(title).font(.system(size: 11, weight: .semibold))
            }
            .foregroundStyle(selectedTab == index ? accent : muted)
            .frame(maxWidth: .infinity)
            .frame(height: 42)
            .background(selectedTab == index ? accent.opacity(0.10) : Color.clear, in: Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityAddTraits(selectedTab == index ? .isSelected : [])
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
    let panel: Color
    let raised: Color
    let muted: Color

    var body: some View {
        HStack(spacing: 13) {
            ZStack {
                RoundedRectangle(cornerRadius: 14)
                    .fill(LinearGradient(
                        colors: [accent.opacity(0.18), accent.opacity(0.06)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    ))
                    .frame(width: 48, height: 48)
                Image(systemName: "app.dashed")
                    .font(.system(size: 21, weight: .medium))
                    .foregroundStyle(accent)
            }
            VStack(alignment: .leading, spacing: 5) {
                Text(item.displayName)
                    .font(.system(size: 14, weight: .semibold))
                    .lineLimit(1)
                    .foregroundStyle(.white)
                Text(item.inspection.summary)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(muted)
                    .lineLimit(1)
                Text("ARCHIVE INSPECTED")
                    .font(.system(size: 8, weight: .bold))
                    .tracking(0.8)
                    .foregroundStyle(accent.opacity(0.9))
            }
            Spacer(minLength: 4)
            Image(systemName: "chevron.right")
                .font(.system(size: 10, weight: .bold))
                .foregroundStyle(muted)
        }
        .padding(13)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(raised, in: RoundedRectangle(cornerRadius: 18))
        .overlay(RoundedRectangle(cornerRadius: 18).stroke(Color.white.opacity(0.045), lineWidth: 1))
        .contentShape(RoundedRectangle(cornerRadius: 18))
    }
}

private struct APKDetailSheet: View {
    @Environment(\.dismiss) private var dismiss
    let item: ImportedAPK
    let accent: Color
    let canvas: Color
    let panel: Color
    let raised: Color
    let muted: Color

    var body: some View {
        VStack(spacing: 0) {
            HStack {
                Text("PACKAGE DETAILS")
                    .font(.system(size: 10, weight: .bold))
                    .tracking(1.5)
                    .foregroundStyle(muted)
                Spacer()
                Button { dismiss() } label: {
                    Image(systemName: "xmark")
                        .font(.system(size: 12, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 34, height: 34)
                        .background(raised, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Close package details")
            }
            .padding(.horizontal, 20)
            .padding(.top, 18)
            .padding(.bottom, 10)

            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 19) {
                    VStack(alignment: .leading, spacing: 9) {
                        Image(systemName: "app.dashed")
                            .font(.system(size: 23, weight: .medium))
                            .foregroundStyle(accent)
                            .frame(width: 54, height: 54)
                            .background(accent.opacity(0.11), in: RoundedRectangle(cornerRadius: 17))
                        Text(item.displayName)
                            .font(.system(size: 25, weight: .bold, design: .rounded))
                            .lineLimit(2)
                            .fixedSize(horizontal: false, vertical: true)
                        Text("Static package report")
                            .font(.subheadline)
                            .foregroundStyle(muted)
                    }
                    VStack(spacing: 0) {
                        detailRow("Archive", value: item.inspection.isValidPackage ? "Valid APK" : "Invalid", symbol: "archivebox")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Manifest", value: item.inspection.hasAndroidManifest ? "Found" : "Missing", symbol: "doc.text")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("DEX files", value: item.inspection.hasDEX ? "\(item.inspection.dexFileCount)" : "None found", symbol: "chevron.left.forwardslash.chevron.right")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Resource table", value: item.inspection.hasResourceTable ? "Found" : "Not found", symbol: "square.grid.3x3")
                        Divider().overlay(Color.white.opacity(0.05))
                        detailRow("Archive entries", value: "\(item.inspection.archiveEntryCount)", symbol: "list.number")
                    }
                    .padding(.horizontal, 15)
                    .background(panel, in: RoundedRectangle(cornerRadius: 18))

                    VStack(alignment: .leading, spacing: 11) {
                        Text("NATIVE ARCHITECTURES")
                            .font(.system(size: 9, weight: .bold))
                            .tracking(1.2)
                            .foregroundStyle(muted)
                        if item.inspection.nativeABIs.isEmpty {
                            Text("No native libraries detected")
                                .font(.subheadline)
                                .foregroundStyle(muted)
                        } else {
                            FlowABIList(values: item.inspection.nativeABIs, accent: accent, raised: raised)
                        }
                    }
                    .padding(16)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(panel, in: RoundedRectangle(cornerRadius: 18))

                    HStack(alignment: .top, spacing: 11) {
                        Image(systemName: "info.circle.fill").foregroundStyle(accent)
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Inspection only")
                                .font(.subheadline.weight(.semibold))
                            Text("Stride has not executed this APK. Android runtime compatibility is not yet available.")
                                .font(.footnote)
                                .foregroundStyle(muted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                    }
                    .padding(15)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(accent.opacity(0.08), in: RoundedRectangle(cornerRadius: 17))
                    .overlay(RoundedRectangle(cornerRadius: 17).stroke(accent.opacity(0.14), lineWidth: 1))
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
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(accent)
                .frame(width: 22)
            Text(title)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(muted)
            Spacer()
            Text(value)
                .font(.system(size: 13, weight: .semibold))
                .multilineTextAlignment(.trailing)
        }
        .padding(.vertical, 14)
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
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    .foregroundStyle(accent)
                    .padding(.horizontal, 11)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(raised, in: Capsule())
            }
        }
    }
}
