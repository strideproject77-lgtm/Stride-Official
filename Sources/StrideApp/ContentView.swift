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
    @AppStorage("stride.selectedTheme.v3") private var themeRawValue = StrideTheme.indigoNavy.rawValue
    @AppStorage("stride.customAccentHex") private var customAccentHex = "#4F46E5"
    @AppStorage("stride.appLayout") private var appLayoutRawValue = "grid"

    private var theme: StrideTheme { StrideTheme(rawValue: themeRawValue) ?? .indigoNavy }
    private var themeIsLight: Bool { theme == .white }
    private var background: Color { theme.background }
    private var panel: Color { theme.panel }
    private var raised: Color { theme.raised }
    private var accent: Color { Color(hex: customAccentHex) ?? theme.accent }
    private var muted: Color { theme.muted }
    private var line: Color { theme == .white ? Color.black.opacity(0.09) : Color.white.opacity(0.075) }

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
                        settingsView
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
        .preferredColorScheme(theme == .white ? .light : .dark)
        }
    }

    private var header: some View {
        HStack(spacing: 11) {
            StrideCoreMark(size: 40, accent: accent)
            VStack(alignment: .leading, spacing: 2) {
                Text("STRIDE")
                    .font(.system(size: 21, weight: .black, design: .rounded))
                    .kerning(2.4)
                Text("ANDROID APP RUNTIME")
                    .font(.system(size: 8, weight: .semibold))
                    .kerning(1.2)
                    .foregroundStyle(muted)
            }
            Spacer()
            if isImporting {
                ProgressView().tint(accent).frame(width: 34, height: 34)
            } else {
                Button { selectedTab = 2 } label: {
                    Image(systemName: "slider.horizontal.3")
                        .font(.system(size: 19, weight: .medium))
                        .foregroundStyle(accent)
                        .frame(width: 38, height: 38)
                        .background(panel, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Settings")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 15)
    }

    private var home: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 20) {
                VStack(alignment: .leading, spacing: 7) {
                    Text("YOUR RUNTIME SPACE")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.6)
                        .foregroundStyle(accent)
                    Text("Good to see you.")
                        .font(.system(size: 29, weight: .bold, design: .rounded))
                        .kerning(-0.7)
                    Text("Your Android apps, organized on iPhone.")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(muted)
                }
                if let featured = library.packages.first {
                    VStack(alignment: .leading, spacing: 12) {
                        HStack {
                            Label("LATEST IMPORT", systemImage: "sparkle")
                                .font(.system(size: 9, weight: .heavy))
                                .foregroundStyle(accent)
                            Spacer()
                            Text("IN YOUR LIBRARY")
                                .font(.system(size: 8, weight: .bold))
                                .foregroundStyle(muted)
                        }
                        HStack(spacing: 12) {
                            APKIconView(data: featured.iconData, accent: accent, size: 58)
                                .background(raised, in: RoundedRectangle(cornerRadius: 16))
                                .clipShape(RoundedRectangle(cornerRadius: 16))
                            VStack(alignment: .leading, spacing: 4) {
                                Text(featured.displayName)
                                    .font(.system(size: 17, weight: .bold, design: .rounded))
                                    .lineLimit(2)
                                Text("Ready in your library")
                                    .font(.system(size: 10, weight: .medium))
                                    .foregroundStyle(muted)
                            }
                            Spacer(minLength: 0)
                        }
                        Button { selectedPackage = featured } label: {
                            HStack {
                                Text("OPEN APP DETAILS")
                                Spacer()
                                Image(systemName: "arrow.up.right")
                            }
                            .font(.system(size: 10, weight: .heavy))
                            .foregroundStyle(theme.buttonForeground)
                            .padding(.horizontal, 14)
                            .frame(height: 42)
                            .background(accent, in: RoundedRectangle(cornerRadius: 12))
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(17)
                    .background(theme.featured, in: RoundedRectangle(cornerRadius: 20))
                    .overlay(RoundedRectangle(cornerRadius: 20).stroke(accent.opacity(0.24), lineWidth: 1))
                } else {
                    VStack(alignment: .leading, spacing: 13) {
                        Text("A clean home for your Android apps.")
                            .font(.system(size: 20, weight: .bold, design: .rounded))
                        Text("Import an APK to inspect its package structure and keep it in your library.")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                        Button { showingImporter = true } label: {
                            Label(isImporting ? "Importing…" : "Import APK", systemImage: "plus")
                                .font(.system(size: 12, weight: .bold))
                                .foregroundStyle(theme.buttonForeground)
                                .frame(maxWidth: .infinity)
                                .frame(height: 46)
                                .background(accent, in: RoundedRectangle(cornerRadius: 13))
                        }
                        .buttonStyle(.plain)
                        .disabled(isImporting)
                    }
                    .padding(19)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(theme.featured, in: RoundedRectangle(cornerRadius: 20))
                    .overlay(RoundedRectangle(cornerRadius: 20).stroke(accent.opacity(0.22), lineWidth: 1))
                }

                HStack {
                    Text("RECENTLY IMPORTED")
                        .font(.system(size: 10, weight: .heavy))
                        .kerning(1.3)
                        .foregroundStyle(muted)
                    Spacer()
                    Text("\(library.packages.count)")
                        .font(.system(size: 11, weight: .semibold, design: .monospaced))
                        .foregroundStyle(muted)
                    if !library.packages.isEmpty {
                        layoutToggle
                        Button("SEE ALL") { selectedTab = 1 }
                            .font(.system(size: 9, weight: .heavy))
                            .foregroundStyle(accent)
                    }
                }
                if library.packages.isEmpty {
                    emptyState
                } else {
                    appCollection(Array(library.packages.prefix(4)))
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 5)
            .padding(.bottom, 22)
        }
    }

    private var libraryView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 15) {
                HStack {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("YOUR LIBRARY")
                            .font(.system(size: 23, weight: .bold, design: .rounded))
                    }
                    Spacer()
                    layoutToggle
                    Button { showingImporter = true } label: {
                        Image(systemName: "plus")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(theme.buttonForeground)
                            .frame(width: 43, height: 43)
                            .background(accent, in: RoundedRectangle(cornerRadius: 14))
                    }
                    .buttonStyle(.plain)
                    .disabled(isImporting)
                    .accessibilityLabel("Import APK")
                }
                HStack(spacing: 9) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(muted)
                    TextField("Search your games…", text: $searchText)
                        .font(.system(size: 12, weight: .medium))
                        .tint(accent)
                        .autocorrectionDisabled()
                }
                .padding(.horizontal, 13)
                .frame(height: 44)
                .background(panel, in: RoundedRectangle(cornerRadius: 13))
                .overlay(RoundedRectangle(cornerRadius: 13).stroke(line, lineWidth: 1))

                HStack {
                    Text("ALL APPS")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.2)
                        .foregroundStyle(muted)
                    Spacer()
                    Text("\(filteredPackages.count) ITEMS")
                        .font(.system(size: 9, weight: .bold, design: .monospaced))
                        .foregroundStyle(muted)
                }
                if filteredPackages.isEmpty {
                    if library.packages.isEmpty {
                        emptyState
                    } else {
                        Text("No apps match “\(searchText)”.")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(muted)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 28)
                    }
                } else {
                    appCollection(filteredPackages)
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 22)
        }
    }

    private var isGridLayout: Bool { appLayoutRawValue == "grid" }

    private var layoutToggle: some View {
        Button {
            appLayoutRawValue = isGridLayout ? "list" : "grid"
        } label: {
            Image(systemName: isGridLayout ? "list.bullet" : "square.grid.2x2")
                .font(.system(size: 15, weight: .semibold))
                .foregroundStyle(accent)
                .frame(width: 38, height: 38)
                .background(panel, in: RoundedRectangle(cornerRadius: 12))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isGridLayout ? "Switch to list view" : "Switch to grid view")
    }

    @ViewBuilder
    private func appCollection(_ items: [ImportedAPK]) -> some View {
        if isGridLayout {
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 10), GridItem(.flexible(), spacing: 10)], spacing: 10) {
                ForEach(items) { item in packageButton(item) }
            }
        } else {
            VStack(spacing: 9) {
                ForEach(items) { item in packageButton(item) }
            }
        }
    }

    private var filteredPackages: [ImportedAPK] {
        guard !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            return library.packages
        }
        return library.packages.filter {
            $0.displayName.localizedCaseInsensitiveContains(searchText)
        }
    }

    private var settingsView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 5) {
                    Text("SETTINGS")
                        .font(.system(size: 23, weight: .bold, design: .rounded))
                    Text("Make Stride feel like yours.")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(muted)
                }
                VStack(alignment: .leading, spacing: 12) {
                    Text("THEME & ACCENT COLOR")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.3)
                        .foregroundStyle(muted)
                    HStack(spacing: 10) {
                        ForEach(["#4F46E5", "#6366F1", "#22C55E", "#F97316", "#EC4899", "#06B6D4", "#EAB308", "#FFFFFF"], id: \.self) { hex in
                            Button { customAccentHex = hex } label: {
                                Circle().fill(Color(hex: hex) ?? .indigo).frame(width: 29, height: 29)
                                    .overlay(Circle().stroke(Color.white.opacity(customAccentHex == hex ? 0.95 : 0.16), lineWidth: customAccentHex == hex ? 2.5 : 1))
                                    .overlay { if customAccentHex == hex { Image(systemName: "checkmark").font(.system(size: 10, weight: .black)).foregroundStyle(hex == "#FFFFFF" ? .black : .white) } }
                            }.buttonStyle(.plain).accessibilityLabel("Accent color \\(hex)")
                        }
                    }
                    ColorPicker("Custom accent", selection: Binding(
                        get: { Color(hex: customAccentHex) ?? theme.accent },
                        set: { customAccentHex = $0.hexString }
                    ), supportsOpacity: false)
                    .font(.system(size: 12, weight: .medium))
                    ForEach(StrideTheme.allCases.filter { $0 != .darkBronze }) { option in
                        Button { themeRawValue = option.rawValue } label: {
                            HStack(spacing: 12) {
                                RoundedRectangle(cornerRadius: 10)
                                    .fill(option.panel)
                                    .frame(width: 43, height: 43)
                                    .overlay {
                                        Image(systemName: "hexagon")
                                            .font(.system(size: 25, weight: .medium))
                                            .foregroundStyle(option.accent)
                                            .overlay {
                                                Text("S")
                                                    .font(.system(size: 11, weight: .black, design: .rounded))
                                                    .foregroundStyle(.primary)
                                            }
                                    }
                                VStack(alignment: .leading, spacing: 3) {
                                    Text(option.title)
                                        .font(.system(size: 12, weight: .bold))
                                        .foregroundStyle(.white)
                                    Text(option.subtitle)
                                        .font(.system(size: 9, weight: .medium))
                                        .foregroundStyle(muted)
                                }
                                Spacer()
                                if theme == option {
                                    Image(systemName: "checkmark.circle.fill")
                                        .font(.system(size: 19))
                                        .foregroundStyle(option.accent)
                                } else {
                                    Circle().stroke(Color.primary.opacity(0.18), lineWidth: 1.2)
                                        .frame(width: 18, height: 18)
                                }
                            }
                            .padding(11)
                            .background(theme == option ? option.accent.opacity(0.09) : panel,
                                        in: RoundedRectangle(cornerRadius: 15))
                            .overlay(RoundedRectangle(cornerRadius: 15)
                                .stroke(theme == option ? option.accent.opacity(0.45) : line, lineWidth: 1))
                        }
                        .buttonStyle(.plain)
                    }
                }
                VStack(alignment: .leading, spacing: 11) {
                    Text("RUNTIME STATUS")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.3)
                        .foregroundStyle(muted)
                    statusCard("APK archive inspection", detail: "Available", symbol: "checkmark.circle.fill", good: true)
                    statusCard("DEX parsing and method loading", detail: "Integrated foundations", symbol: "curlybraces", good: true)
                    statusCard("ART / Android framework", detail: "Partial implementation · not full compatibility", symbol: "cpu", good: false)
                    statusCard("Native libraries and graphics", detail: "Work in progress", symbol: "square.stack.3d.up", good: false)
                }
                Text("STRIDE · EXPERIMENTAL BUILD")
                    .font(.system(size: 8, weight: .heavy))
                    .kerning(1.2)
                    .foregroundStyle(muted.opacity(0.75))
                    .frame(maxWidth: .infinity)
                    .padding(.top, 2)
            }
            .padding(.horizontal, 18)
            .padding(.top, 10)
            .padding(.bottom, 24)
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
            tab("gearshape.fill", title: "Settings", index: 2)
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
            if isGridLayout {
                APKGridCard(item: item, accent: accent, panel: panel, raised: raised, muted: muted, line: line)
            } else {
                APKRow(item: item, accent: accent, panel: panel, raised: raised, muted: muted, line: line)
            }
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


private extension Color {
    init?(hex: String) {
        let value = hex.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        guard value.count == 6, let rgb = UInt64(value, radix: 16) else { return nil }
        self.init(.sRGB, red: Double((rgb >> 16) & 255) / 255, green: Double((rgb >> 8) & 255) / 255, blue: Double(rgb & 255) / 255, opacity: 1)
    }
    var hexString: String {
        guard let c = UIColor(self).cgColor.components, c.count >= 3 else { return "#4F46E5" }
        return String(format: "#%02X%02X%02X", Int(c[0] * 255), Int(c[1] * 255), Int(c[2] * 255))
    }
}

private enum StrideTheme: String, CaseIterable, Identifiable {
    case blackLime
    case graphiteViolet
    case darkBronze
    case indigoNavy
    case white

    var id: String { rawValue }
    var title: String {
        switch self {
        case .blackLime: return "Black + Lime"
        case .graphiteViolet: return "Graphite + Violet"
        case .darkBronze: return "Dark Bronze"
        case .indigoNavy: return "Indigo Navy"
        case .white: return "White"
        }
    }
    var subtitle: String {
        switch self {
        case .blackLime: return "Sharp contrast with a bright lime accent"
        case .graphiteViolet: return "Cool graphite surfaces and soft violet"
        case .darkBronze: return "Warm, understated bronze finish"
        case .indigoNavy: return "Deep navy with royal purple accents"
        case .white: return "Bright, clean surfaces with crisp dark text"
        }
    }
    var background: Color {
        switch self {
        case .blackLime: return Color(red: 0.025, green: 0.027, blue: 0.032)
        case .graphiteViolet: return Color(red: 0.055, green: 0.050, blue: 0.075)
        case .darkBronze: return Color(red: 0.045, green: 0.030, blue: 0.022)
        case .indigoNavy: return Color(red: 10/255, green: 13/255, blue: 24/255)
        case .white: return Color(red: 0.965, green: 0.970, blue: 0.980)
        }
    }
    var panel: Color {
        switch self {
        case .blackLime: return Color(red: 0.075, green: 0.078, blue: 0.085)
        case .graphiteViolet: return Color(red: 0.105, green: 0.095, blue: 0.135)
        case .darkBronze: return Color(red: 0.115, green: 0.082, blue: 0.058)
        case .indigoNavy: return Color(red: 21/255, green: 25/255, blue: 40/255)
        case .white: return Color.white
        }
    }
    var raised: Color {
        switch self {
        case .blackLime: return Color(red: 0.115, green: 0.120, blue: 0.130)
        case .graphiteViolet: return Color(red: 0.165, green: 0.145, blue: 0.205)
        case .darkBronze: return Color(red: 0.175, green: 0.125, blue: 0.085)
        case .indigoNavy: return Color(red: 29/255, green: 34/255, blue: 53/255)
        case .white: return Color(red: 0.925, green: 0.935, blue: 0.955)
        }
    }
    var accent: Color {
        switch self {
        case .blackLime: return Color(red: 0.77, green: 0.96, blue: 0.48)
        case .graphiteViolet: return Color(red: 0.73, green: 0.64, blue: 1.0)
        case .darkBronze: return Color(red: 0.91, green: 0.70, blue: 0.47)
        case .indigoNavy: return Color(red: 79/255, green: 70/255, blue: 229/255)
        case .white: return Color(red: 0.22, green: 0.34, blue: 0.56)
        }
    }
    var muted: Color {
        self == .indigoNavy
            ? Color(red: 148/255, green: 163/255, blue: 184/255)
            : Color(red: 0.59, green: 0.60, blue: 0.64)
    }
    var featured: LinearGradient {
        switch self {
        case .blackLime:
            return LinearGradient(colors: [Color(red: 0.16, green: 0.21, blue: 0.12), panel], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .graphiteViolet:
            return LinearGradient(colors: [Color(red: 0.20, green: 0.17, blue: 0.29), panel], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .darkBronze:
            return LinearGradient(colors: [Color(red: 0.23, green: 0.16, blue: 0.10), panel], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .indigoNavy:
            return LinearGradient(colors: [Color(red: 99/255, green: 102/255, blue: 241/255).opacity(0.22), panel], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .white:
            return LinearGradient(colors: [Color(red: 0.91, green: 0.94, blue: 0.99), panel], startPoint: .topLeading, endPoint: .bottomTrailing)
        }
    }
    var buttonForeground: Color {
        switch self {
        case .blackLime: return Color(red: 0.06, green: 0.08, blue: 0.04)
        case .graphiteViolet: return Color(red: 0.08, green: 0.06, blue: 0.12)
        case .darkBronze: return Color(red: 0.13, green: 0.08, blue: 0.04)
        case .indigoNavy: return .white
        case .white: return Color.white
        }
    }
}

private struct StrideCoreMark: View {
    let size: CGFloat
    let accent: Color

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.29)
                .fill(LinearGradient(colors: [accent.opacity(0.18), accent.opacity(0.045)],
                                     startPoint: .topLeading, endPoint: .bottomTrailing))
            Image(systemName: "speedometer")
                .font(.system(size: size * 0.54, weight: .bold))
                .foregroundStyle(accent)
            Image(systemName: "bolt.fill")
                .font(.system(size: size * 0.19, weight: .black))
                .foregroundStyle(.white)
                .offset(x: size * 0.17, y: size * 0.16)
        }
        .frame(width: size, height: size)
        .accessibilityLabel("Stride Dynamic Speed logo")
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

private struct APKGridCard: View {
    let item: ImportedAPK
    let accent: Color
    let panel: Color
    let raised: Color
    let muted: Color
    let line: Color

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Spacer(minLength: 0)
                APKIconView(data: item.iconData, accent: accent, size: 66)
                    .background(raised, in: RoundedRectangle(cornerRadius: 19))
                    .clipShape(RoundedRectangle(cornerRadius: 19))
                Spacer(minLength: 0)
            }
            Text(item.displayName)
                .font(.system(size: 12, weight: .bold, design: .rounded))
                .foregroundStyle(.white)
                .lineLimit(2)
                .frame(maxWidth: .infinity, minHeight: 32, alignment: .leading)
            Text(item.inspection.summary)
                .font(.system(size: 9, weight: .medium))
                .foregroundStyle(muted)
                .lineLimit(2)
                .frame(maxWidth: .infinity, alignment: .leading)
            HStack(spacing: 5) {
                Circle().fill(accent).frame(width: 5, height: 5)
                Text("IMPORTED")
                    .font(.system(size: 7, weight: .heavy))
                    .kerning(0.7)
                    .foregroundStyle(accent)
                Spacer(minLength: 0)
                Image(systemName: "arrow.up.right")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(muted)
            }
        }
        .padding(12)
        .frame(maxWidth: .infinity, minHeight: 170, alignment: .topLeading)
        .background(panel, in: RoundedRectangle(cornerRadius: 17))
        .overlay(RoundedRectangle(cornerRadius: 17).stroke(line, lineWidth: 1))
        .contentShape(RoundedRectangle(cornerRadius: 17))
    }
}

private struct APKDetailSheet: View {
    @State private var showingLogShare = false
    @State private var showingExecutionScreen = false
    @Environment(\.dismiss) private var dismiss
    let item: ImportedAPK
    @State private var isExecuting = false
    @State private var executionResult: APKExecutionResult?
    @State private var executionError: String?
    @State private var sessionLogLines: [String] = []
    @State private var logExportURL: URL?
    @State private var lastRunLabel = "No previous launch recorded"
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
                        Divider().overlay(Color.primary.opacity(0.06))
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
                        showingExecutionScreen = true
                        Task {
                            isExecuting = true
                            executionError = nil
                            executionResult = nil
                            sessionLogLines = []
                            appendSessionLog("[SESSION] Force execution requested for \(item.displayName)")
                            appendSessionLog("[SESSION] Started: \(Date().formatted(date: .abbreviated, time: .standard))")
                            do {
                                let backend = StrideArtExecutionBackend()
                                let res = try await backend.launch(packageAt: item.fileURL) { line in
                                    Task { @MainActor in appendSessionLog(line) }
                                }
                                executionResult = res
                                lastRunLabel = "Completed · \(Date().formatted(date: .abbreviated, time: .shortened))"
                                appendSessionLog("[SESSION] Launch attempt completed with lifecycle \(res.lifecycleState)")
                            } catch {
                                executionError = error.localizedDescription
                                lastRunLabel = "Failed · \(Date().formatted(date: .abbreviated, time: .shortened))"
                                appendSessionLog("[SESSION] FAILED: \(error.localizedDescription)")
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
                                Text("Force Execute in ART Runtime")
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

                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text("How was this app run last time?")
                                    .font(.system(size: 13, weight: .bold))
                                    .foregroundStyle(.white)
                                Text(lastRunLabel)
                                    .font(.system(size: 10, weight: .medium))
                                    .foregroundStyle(muted)
                            }
                            Spacer()
                            Image(systemName: "clock.arrow.circlepath").foregroundStyle(accent)
                        }
                        Text("Session Logs").font(.system(size: 11, weight: .bold)).foregroundStyle(.white)
                        ScrollView {
                            VStack(alignment: .leading, spacing: 5) {
                                if sessionLogLines.isEmpty {
                                    Text("No launch logs yet. Force Execute to record the next attempt.")
                                        .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                                } else {
                                    ForEach(Array(sessionLogLines.suffix(80).enumerated()), id: \.offset) { _, line in
                                        Text(line)
                                            .font(.system(size: 9, weight: .regular, design: .monospaced))
                                            .foregroundStyle(line.contains("[ERROR]") || line.contains("[CRASH]") || line.contains("FAILED") ? Color.red.opacity(0.95) : muted)
                                            .frame(maxWidth: .infinity, alignment: .leading)
                                            .textSelection(.enabled)
                                    }
                                }
                            }
                        }
                        .frame(maxHeight: 180)
                        .padding(10)
                        .background(canvas, in: RoundedRectangle(cornerRadius: 11))
                        if let logExportURL {
                            Button {
                                showingLogShare = true
                            } label: {
                                Label("Export Logs (.txt)", systemImage: "square.and.arrow.up")
                                    .font(.system(size: 11, weight: .semibold)).foregroundStyle(accent)
                            }
                            .sheet(isPresented: $showingLogShare) {
                                ActivityViewController(activityItems: [logExportURL])
                            }
                        }
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))

                    if let res = executionResult {
                        VStack(alignment: .leading, spacing: 7) {
                            HStack {
                                Image(systemName: "checkmark.circle.fill").foregroundStyle(accent)
                                Text("Runtime report").font(.system(size: 13, weight: .bold))
                            }
                            Text("Launch result: \(res.statusCode == 0 ? "Lifecycle reached" : "Finished") · Lifecycle: \(res.lifecycleState)")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(muted)
                            Text("Classes: \(res.totalClassesLoaded) · Instructions: \(res.instructionsExecuted) · Missing APIs: \(res.missingApisCount)")
                                .font(.system(size: 10, weight: .medium))
                                .foregroundStyle(muted)
                            Text("Performance: \(String(format: "%.1f", res.averageFps)) FPS · RAM: \(res.heapAllocatedBytes / 1024) KB")
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
        .overlay {
            if showingExecutionScreen {
                executionScreen
                    .transition(.opacity)
                    .zIndex(10)
            }
        }
        .onAppear(perform: loadPreviousSession)
    }

    private var executionScreen: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 0) {
                HStack(spacing: 12) {
                    APKIconView(data: item.iconData, accent: accent, size: 46)
                        .background(Color.white.opacity(0.08), in: RoundedRectangle(cornerRadius: 12))
                    VStack(alignment: .leading, spacing: 4) {
                        Text(item.displayName)
                            .font(.system(size: 17, weight: .bold, design: .rounded))
                            .foregroundStyle(.white)
                            .lineLimit(1)
                        Text("STRIDE · ANDROID EXECUTION")
                            .font(.system(size: 9, weight: .heavy))
                            .kerning(1.1)
                            .foregroundStyle(.white.opacity(0.48))
                    }
                    Spacer()
                    Button {
                        showingExecutionScreen = false
                    } label: {
                        Image(systemName: "xmark")
                            .font(.system(size: 12, weight: .bold))
                            .foregroundStyle(.white)
                            .frame(width: 36, height: 36)
                            .background(Color.white.opacity(0.1), in: Circle())
                    }
                    .buttonStyle(.plain)
                }
                .padding(.bottom, 24)

                HStack(spacing: 8) {
                    Circle()
                        .fill(isExecuting ? Color.orange : (executionError == nil && executionResult != nil ? Color.green : Color.red))
                        .frame(width: 8, height: 8)
                    Text(isExecuting ? "STARTING RUNTIME" : (executionError == nil && executionResult != nil ? "LAUNCH ATTEMPT COMPLETE" : (executionError != nil ? "LAUNCH FAILED" : "READY")))
                        .font(.system(size: 10, weight: .heavy))
                        .kerning(1.2)
                        .foregroundStyle(.white.opacity(0.78))
                    Spacer()
                    Text("\(sessionLogLines.count) EVENTS")
                        .font(.system(size: 9, weight: .semibold, design: .monospaced))
                        .foregroundStyle(.white.opacity(0.42))
                }
                .padding(.bottom, 12)

                if let result = executionResult {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("ACTIVITY LIFECYCLE")
                            .font(.system(size: 9, weight: .bold))
                            .foregroundStyle(.white.opacity(0.45))
                        Text(result.lifecycleState)
                            .font(.system(size: 22, weight: .bold, design: .rounded))
                            .foregroundStyle(.white)
                        Text("\(result.instructionsExecuted) instructions · \(result.totalClassesLoaded) classes · \(result.missingApisCount) missing APIs")
                            .font(.system(size: 10, weight: .medium, design: .monospaced))
                            .foregroundStyle(.white.opacity(0.55))
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color.white.opacity(0.07), in: RoundedRectangle(cornerRadius: 14))
                    .padding(.bottom, 18)
                } else if let error = executionError {
                    Text(error)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(Color.red.opacity(0.95))
                        .padding(14)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background(Color.red.opacity(0.1), in: RoundedRectangle(cornerRadius: 12))
                        .padding(.bottom, 18)
                } else if isExecuting {
                    HStack(spacing: 10) {
                        ProgressView().tint(.white)
                        Text("Preparing APK and starting runtime…")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.white.opacity(0.7))
                    }
                    .padding(.bottom, 18)
                }

                Text("LIVE SESSION LOG")
                    .font(.system(size: 9, weight: .heavy))
                    .kerning(1.2)
                    .foregroundStyle(.white.opacity(0.48))
                    .padding(.bottom, 9)
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 7) {
                        ForEach(Array(sessionLogLines.enumerated()), id: \.offset) { _, line in
                            Text(line)
                                .font(.system(size: 10, weight: .regular, design: .monospaced))
                                .foregroundStyle(line.contains("[ERROR]") || line.contains("[CRASH]") || line.contains("FAILED") ? Color.red : Color.white.opacity(0.76))
                                .frame(maxWidth: .infinity, alignment: .leading)
                                .textSelection(.enabled)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                }
                .padding(13)
                .background(Color.white.opacity(0.055), in: RoundedRectangle(cornerRadius: 12))

                HStack(spacing: 12) {
                    Button {
                        showingExecutionScreen = false
                    } label: {
                        Text("BACK TO DETAILS")
                            .font(.system(size: 10, weight: .bold))
                            .foregroundStyle(.white)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 14)
                            .background(Color.white.opacity(0.12), in: RoundedRectangle(cornerRadius: 11))
                    }
                    .buttonStyle(.plain)
                    if logExportURL != nil {
                        Button {
                            showingLogShare = true
                        } label: {
                            Label("EXPORT TXT", systemImage: "square.and.arrow.up")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(.black)
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 14)
                                .background(.white, in: RoundedRectangle(cornerRadius: 11))
                        }
                        .buttonStyle(.plain)
                    }
                }
                .padding(.top, 14)
            }
            .padding(20)
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showingLogShare) {
            if let logExportURL {
                ActivityViewController(activityItems: [logExportURL])
            }
        }
    }

    private func logFileURL() -> URL {
        let safeName = item.displayName.replacingOccurrences(of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression)
        let folder = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
            .appendingPathComponent("StrideLogs", isDirectory: true)
        try? FileManager.default.createDirectory(at: folder, withIntermediateDirectories: true)
        return folder.appendingPathComponent("\(safeName)-latest-session.txt")
    }

    private func appendSessionLog(_ message: String) {
        let stamp = ISO8601DateFormatter().string(from: Date())
        sessionLogLines.append("[\(stamp)] \(message)")
        let content = sessionLogLines.joined(separator: "\n") + "\n"
        let url = logFileURL()
        do {
            try content.write(to: url, atomically: true, encoding: .utf8)
            logExportURL = url
        } catch {
            executionError = "Could not save session log: \(error.localizedDescription)"
        }
    }

    private func loadPreviousSession() {
        let url = logFileURL()
        guard let content = try? String(contentsOf: url, encoding: .utf8), !content.isEmpty else { return }
        sessionLogLines = content.components(separatedBy: .newlines).filter { !$0.isEmpty }
        logExportURL = url
        if let last = sessionLogLines.last { lastRunLabel = "Saved session · \(last)" }
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
                    .padding(size * 0.07)
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


private struct ActivityViewController: UIViewControllerRepresentable {
    let activityItems: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: activityItems, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}
