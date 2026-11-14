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
    @State private var pendingDelete: ImportedAPK?
    @State private var searchText = ""
    @State private var showingAbout = false
    @AppStorage("stride.appLayout") private var appLayoutRawValue = "grid"
    @AppStorage("stride.accentColorHex") private var accentColorHex = "E8AB64"

    private var background: Color { .black }
    private var panel: Color { .black }
    private var raised: Color { Color(red: 0.075, green: 0.080, blue: 0.090) }
    private var accent: Color { Color(hex: accentColorHex) ?? Color(red: 0.91, green: 0.67, blue: 0.39) }
    private var muted: Color { Color(red: 0.59, green: 0.60, blue: 0.64) }
    private var line: Color { Color.white.opacity(0.075) }

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
                .fullScreenCover(item: $selectedPackage) { item in
                    APKDetailSheet(item: item, accent: accent, canvas: background, panel: panel, raised: raised, muted: muted)
                        .interactiveDismissDisabled(true)
                }
                .fullScreenCover(isPresented: $showingAbout) {
                    AboutStrideView(accent: accent, panel: panel, muted: muted)
                }
                Spacer(minLength: 12)
                tabBar
            }
            .overlay(alignment: .bottomTrailing) {
                if !isImporting {
                    Button { showingImporter = true } label: {
                        Image(systemName: "plus")
                            .font(.system(size: 21, weight: .bold))
                            .foregroundStyle(Color.black)
                            .frame(width: 58, height: 58)
                            .background(accent, in: Circle())
                            .overlay(Circle().stroke(Color.white.opacity(0.18), lineWidth: 1))
                            .shadow(color: .black.opacity(0.30), radius: 12, y: 5)
                    }
                    .buttonStyle(.plain)
                    .accessibilityLabel("Import Android Package")
                    .padding(.trailing, 19)
                    .padding(.bottom, 82)
                }
            }
        }
        .confirmationDialog("Delete this app from Stride?", isPresented: Binding(
            get: { pendingDelete != nil },
            set: { if !$0 { pendingDelete = nil } }
        ), titleVisibility: .visible) {
            Button("Delete App", role: .destructive) {
                if let item = pendingDelete {
                    do { try library.deletePackage(item) }
                    catch {
                        importAlertTitle = "Delete failed"
                        importError = error.localizedDescription
                    }
                }
                pendingDelete = nil
            }
            Button("Cancel", role: .cancel) { pendingDelete = nil }
        } message: {
            Text(pendingDelete?.displayName ?? "")
        }
        .alert(importAlertTitle, isPresented: Binding(
            get: { importError != nil },
            set: { if !$0 { importError = nil } }
        )) {
            Button("OK", role: .cancel) { importError = nil }
        } message: {
            Text(importError ?? "Please try another Android package file.")
        }
        .preferredColorScheme(.dark)
    }

    private var header: some View {
        HStack(spacing: 10) {
            Text("Stride")
                .font(.system(size: 28, weight: .black, design: .rounded))
                .tracking(-1.15)
                .foregroundStyle(.primary)
            Spacer()
            if isImporting {
                ProgressView().tint(accent).frame(width: 34, height: 34)
            } else {
                Button { selectedTab = 1 } label: {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 17, weight: .medium))
                        .foregroundStyle(accent)
                        .frame(width: 37, height: 37)
                        .background(panel, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Search games")
                Button { selectedTab = 2 } label: {
                    Image(systemName: "gearshape")
                        .font(.system(size: 18, weight: .medium))
                        .foregroundStyle(accent)
                        .frame(width: 37, height: 37)
                        .background(panel, in: Circle())
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Settings")
            }
        }
        .padding(.horizontal, 20)
        .padding(.top, 12)
        .padding(.bottom, 13)
    }

    private var home: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 19) {
                HStack(spacing: 9) {
                    Image(systemName: "magnifyingglass")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(muted)
                    TextField("Search your games...", text: $searchText)
                        .font(.system(size: 13, weight: .medium))
                        .tint(accent)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Search your imported Android apps")
                        .accessibilityHint("Filters the Home library as you type")
                    if !searchText.isEmpty {
                        Button { searchText = "" } label: {
                            Image(systemName: "xmark.circle.fill").foregroundStyle(muted)
                        }.buttonStyle(.plain)
                    }
                }
                .padding(.horizontal, 14)
                .frame(height: 48)
                .background(panel, in: RoundedRectangle(cornerRadius: 15))
                .overlay(RoundedRectangle(cornerRadius: 15).stroke(line, lineWidth: 1))

                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 25)
                        .fill(LinearGradient(
                            colors: [Color(red: 0.29, green: 0.19, blue: 0.14), Color(red: 0.18, green: 0.12, blue: 0.10), Color(red: 0.08, green: 0.07, blue: 0.07)],
                            startPoint: .topLeading, endPoint: .bottomTrailing
                        ))
                    Circle()
                        .fill(Color(red: 0.78, green: 0.43, blue: 0.22).opacity(0.24))
                        .frame(width: 180, height: 180)
                        .blur(radius: 28)
                        .offset(x: 205, y: -48)
                    HStack(spacing: 4) {
                        VStack(alignment: .leading, spacing: 10) {
                            Text("WELCOME TO STRIDE")
                                .font(.system(size: 9, weight: .heavy))
                                .kerning(1.5)
                                .foregroundStyle(Color.white.opacity(0.72))
                            Text("Welcome to Stride")
                                .font(.system(size: 23, weight: .bold, design: .rounded))
                                .foregroundStyle(.white)
                                .fixedSize(horizontal: false, vertical: true)
                            Text("Run Android Apps on iOS")
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(Color.white.opacity(0.76))
                            Button {
                                if library.packages.isEmpty { showingImporter = true } else { selectedTab = 1 }
                            } label: {
                                Image(systemName: "arrow.right")
                                    .font(.system(size: 15, weight: .bold))
                                    .foregroundStyle(Color(red: 0.22, green: 0.14, blue: 0.10))
                                    .frame(width: 39, height: 39)
                                    .background(.white, in: Circle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(library.packages.isEmpty ? "Import your first APK" : "Open your library")
                        }
                        Spacer(minLength: 0)
                        ZStack {
                            RoundedRectangle(cornerRadius: 23)
                                .fill(Color.white.opacity(0.10))
                                .frame(width: 91, height: 158)
                                .overlay(RoundedRectangle(cornerRadius: 23).stroke(Color.white.opacity(0.34), lineWidth: 1.3))
                            VStack(spacing: 10) {
                                Capsule().fill(Color.white.opacity(0.6)).frame(width: 27, height: 3)
                                ZStack {
                                    RoundedRectangle(cornerRadius: 13).fill(LinearGradient(colors: [Color(red: 0.78, green: 0.48, blue: 0.25), Color(red: 0.40, green: 0.25, blue: 0.16)], startPoint: .topLeading, endPoint: .bottomTrailing))
                                    Image(systemName: "hexagon.fill")
                                        .font(.system(size: 36, weight: .medium))
                                        .foregroundStyle(Color.white.opacity(0.9))
                                        .overlay {
                                            Text("S").font(.system(size: 17, weight: .black, design: .rounded)).foregroundStyle(Color(red: 0.22, green: 0.14, blue: 0.10))
                                        }
                                }
                                .frame(width: 57, height: 65)
                                HStack(spacing: 5) {
                                    Circle().fill(Color.white.opacity(0.75)).frame(width: 5, height: 5)
                                    Capsule().fill(Color.white.opacity(0.35)).frame(width: 27, height: 4)
                                }
                            }
                        }
                        .rotationEffect(.degrees(7))
                        .shadow(color: .black.opacity(0.25), radius: 16, y: 9)
                        .padding(.trailing, 4)
                    }
                    .padding(19)
                }
                .frame(height: 205)
                .clipShape(RoundedRectangle(cornerRadius: 25))

                HStack {
                    Text("RECENTLY PLAYED")
                        .font(.system(size: 10, weight: .heavy))
                        .kerning(1.25)
                        .foregroundStyle(muted)
                    Spacer()
                    Text("PLAY HISTORY")
                        .font(.system(size: 8, weight: .bold))
                        .kerning(0.8)
                        .foregroundStyle(muted.opacity(0.75))
                }
                if let recent = recentlyPlayedPackages.first {
                    Button { selectedPackage = recent } label: {
                        HStack(spacing: 12) {
                            APKIconView(data: recent.iconData, accent: accent, size: 45)
                            VStack(alignment: .leading, spacing: 4) {
                                Text(recent.displayName).font(.system(size: 13, weight: .bold)).foregroundStyle(.white)
                                Text("Last played · \(lastPlayedLabel(for: recent))").font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                            }
                            Spacer()
                            Image(systemName: "arrow.up.right").font(.system(size: 11, weight: .bold)).foregroundStyle(accent)
                        }
                        .padding(13).background(panel, in: RoundedRectangle(cornerRadius: 16))
                        .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))
                    }
                    .buttonStyle(.plain)
                } else {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Your recently played apps will appear here.")
                            .font(.system(size: 12, weight: .semibold)).foregroundStyle(.white.opacity(0.82))
                        Text("Launch an app to start your play history.")
                            .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(15).background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))
                }

                HStack {
                    Text("YOUR GAMES")
                        .font(.system(size: 10, weight: .heavy))
                        .kerning(1.25)
                        .foregroundStyle(muted)
                    Spacer()
                    Text("\(filteredPackages.count)")
                        .font(.system(size: 10, weight: .semibold, design: .monospaced))
                        .foregroundStyle(muted)
                    Button("SEE ALL  →") { selectedTab = 1 }
                        .font(.system(size: 9, weight: .heavy))
                        .foregroundStyle(accent)
                }
                if filteredPackages.isEmpty {
                    VStack(spacing: 10) {
                        Image(systemName: "square.grid.2x2")
                            .font(.system(size: 25, weight: .medium))
                            .foregroundStyle(accent)
                        Text(searchText.isEmpty ? "Your library is empty" : "No games match your search")
                            .font(.system(size: 13, weight: .semibold))
                        Text(searchText.isEmpty ? "Import an APK to add your own apps." : "Try another app name.")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(muted)
                        if searchText.isEmpty {
                            Button { showingImporter = true } label: {
                                Label("Import Android Package", systemImage: "plus")
                                    .font(.system(size: 11, weight: .bold))
                                    .foregroundStyle(Color.black)
                                    .padding(.horizontal, 16)
                                    .padding(.vertical, 11)
                                    .background(accent, in: Capsule())
                            }
                            .buttonStyle(.plain)
                            .disabled(isImporting)
                        } else {
                            Button { searchText = "" } label: {
                                Label("Clear search", systemImage: "xmark.circle.fill")
                                    .font(.system(size: 11, weight: .bold))
                                    .foregroundStyle(accent)
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 10)
                                    .background(accent.opacity(0.10), in: Capsule())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 25)
                    .background(panel, in: RoundedRectangle(cornerRadius: 17))
                } else {
                    appCollection(Array(filteredPackages.prefix(4)))
                }
            }
            .padding(.horizontal, 18)
            .padding(.top, 7)
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
                }
                HStack(spacing: 9) {
                    Image(systemName: "magnifyingglass")
                        .foregroundStyle(muted)
                    TextField("Search your games…", text: $searchText)
                        .font(.system(size: 12, weight: .medium))
                        .tint(accent)
                        .autocorrectionDisabled()
                        .accessibilityLabel("Search your imported Android apps")
                        .accessibilityHint("Filters the Library as you type")
                    if !searchText.isEmpty {
                        Button { searchText = "" } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(muted)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Clear library search")
                    }
                }
                .padding(.horizontal, 13)
                .frame(height: 44)
                .background(panel, in: RoundedRectangle(cornerRadius: 13))
                .overlay(RoundedRectangle(cornerRadius: 13).stroke(line, lineWidth: 1))

                if !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    HStack(spacing: 6) {
                        Image(systemName: "line.3.horizontal.decrease.circle")
                            .font(.system(size: 9, weight: .semibold))
                        Text("FILTERED RESULTS")
                            .font(.system(size: 8, weight: .heavy))
                            .kerning(1.0)
                        Spacer()
                        Text("\(filteredPackages.count) OF \(library.packages.count)")
                            .font(.system(size: 9, weight: .bold, design: .monospaced))
                    }
                    .foregroundStyle(muted)
                    .accessibilityElement(children: .combine)
                    .accessibilityLabel("\(filteredPackages.count) matching apps out of \(library.packages.count)")
                }

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
                        VStack(spacing: 10) {
                            Image(systemName: "magnifyingglass")
                                .font(.system(size: 22, weight: .medium))
                                .foregroundStyle(accent)
                            Text("No apps match “\(searchText)”.")
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(muted)
                            Button { searchText = "" } label: {
                                Label("Clear search", systemImage: "xmark.circle.fill")
                                    .font(.system(size: 11, weight: .bold))
                                    .foregroundStyle(accent)
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 10)
                                    .background(accent.opacity(0.10), in: Capsule())
                            }
                            .buttonStyle(.plain)
                        }
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
            LazyVGrid(columns: [GridItem(.flexible(), spacing: 9), GridItem(.flexible(), spacing: 9), GridItem(.flexible(), spacing: 9)], spacing: 11) {
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

    private var recentlyPlayedPackages: [ImportedAPK] {
        library.packages
            .filter { UserDefaults.standard.object(forKey: lastPlayedKey(for: $0)) != nil }
            .sorted {
                let a = UserDefaults.standard.object(forKey: lastPlayedKey(for: $0)) as? Date ?? .distantPast
                let b = UserDefaults.standard.object(forKey: lastPlayedKey(for: $1)) as? Date ?? .distantPast
                return a > b
            }
            .prefix(3).map { $0 }
    }

    private func lastPlayedKey(for item: ImportedAPK) -> String {
        "stride.lastPlayed.\(item.packageDirectoryURL.standardizedFileURL.path)"
    }

    private func lastPlayedLabel(for item: ImportedAPK) -> String {
        guard let date = UserDefaults.standard.object(forKey: lastPlayedKey(for: item)) as? Date else { return "Not played yet" }
        return date.formatted(date: .abbreviated, time: .shortened)
    }

    private var settingsView: some View {
        ScrollView(showsIndicators: false) {
            VStack(alignment: .leading, spacing: 18) {
                VStack(alignment: .leading, spacing: 5) {
                    Text("SETTINGS")
                        .font(.system(size: 23, weight: .bold, design: .rounded))
                    Text("Manage your Stride experience.")
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(muted)
                }
                VStack(alignment: .leading, spacing: 12) {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("ACCENT COLOR")
                            .font(.system(size: 9, weight: .heavy))
                            .kerning(1.3)
                            .foregroundStyle(muted)
                        Text("Change button, icon and highlight colors")
                            .font(.system(size: 11, weight: .medium))
                            .foregroundStyle(muted)
                    }
                    LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 4), spacing: 12) {
                        ForEach(StrideAccentPalette.colors) { option in
                            let color = Color(hex: option.hex) ?? accent
                            Button {
                                accentColorHex = option.hex
                            } label: {
                                VStack(spacing: 7) {
                                    ZStack {
                                        Circle().fill(color).frame(width: 31, height: 31)
                                        if accentColorHex == option.hex {
                                            Circle().stroke(Color.white, lineWidth: 2).frame(width: 38, height: 38)
                                            Image(systemName: "checkmark")
                                                .font(.system(size: 12, weight: .black))
                                                .foregroundStyle(Color.black.opacity(0.82))
                                        }
                                    }
                                    Text(option.name)
                                        .font(.system(size: 9, weight: .semibold))
                                        .foregroundStyle(.white.opacity(0.88))
                                        .lineLimit(1)
                                }
                                .frame(maxWidth: .infinity)
                                .padding(.vertical, 10)
                                .background(raised.opacity(accentColorHex == option.hex ? 1 : 0.55), in: RoundedRectangle(cornerRadius: 12))
                                .overlay(RoundedRectangle(cornerRadius: 12).stroke(accentColorHex == option.hex ? color.opacity(0.75) : line, lineWidth: 1))
                                .contentShape(RoundedRectangle(cornerRadius: 12))
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("\(option.name) accent color")
                            .accessibilityAddTraits(accentColorHex == option.hex ? .isSelected : [])
                        }
                    }
                    Text("Your choice is saved automatically. App backgrounds stay black.")
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(muted)
                }
                .padding(14)
                .background(panel, in: RoundedRectangle(cornerRadius: 16))
                .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))

                Button { showingAbout = true } label: {
                    HStack(spacing: 13) {
                        Image(systemName: "info.circle.fill").font(.system(size: 19, weight: .semibold)).foregroundStyle(accent)
                            .frame(width: 44, height: 44).background(accent.opacity(0.13), in: RoundedRectangle(cornerRadius: 13))
                        VStack(alignment: .leading, spacing: 4) {
                            Text("About Stride").font(.system(size: 14, weight: .bold)).foregroundStyle(.white)
                            Text("Project details, runtime scope and build information").font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                        }
                        Spacer()
                        Image(systemName: "chevron.right").font(.system(size: 12, weight: .bold)).foregroundStyle(muted)
                    }
                    .padding(14).background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(line, lineWidth: 1))
                }
                .buttonStyle(.plain)
                .accessibilityLabel("About Stride")

                VStack(alignment: .leading, spacing: 11) {
                    Text("RUNTIME STATUS")
                        .font(.system(size: 9, weight: .heavy))
                        .kerning(1.3)
                        .foregroundStyle(muted)
                    statusCard("APK archive inspection", detail: "Available", symbol: "checkmark.circle.fill", good: true)
                    statusCard("DEX parsing and method loading", detail: "Integrated foundations", symbol: "curlybraces", good: true)
                    statusCard("Manifest → Context bootstrap", detail: "Parsed package identity initializes JNI Context", symbol: "app.badge", good: true)
                    statusCard("PackageInfo metadata mapping", detail: "Versions, SDK levels, permissions and components", symbol: "shippingbox", good: true)
                    statusCard("PackageManager lookup", detail: "Manifest-backed current-app package query", symbol: "magnifyingglass", good: true)
                    statusCard("Guest Java PackageManager", detail: "getPackageInfo + getApplicationInfo JNI bridge", symbol: "shippingbox.fill", good: true)
                    statusCard("Context app storage paths", detail: "Files, cache, no-backup and code-cache paths · no disk persistence", symbol: "folder", good: true)
                    statusCard("Launcher Intent resolution", detail: "Manifest-backed MAIN / LAUNCHER Intent and ComponentName", symbol: "arrow.up.right.square", good: true)
                    statusCard("Intent construction and mutation", detail: "Action, package, class, categories and flags", symbol: "arrow.left.arrow.right", good: true)
                    statusCard("Intent extras", detail: "String, integer, long and boolean values with Bundle reads", symbol: "tray.full", good: true)
                    statusCard("URI and Intent data", detail: "android.net.Uri parsing, scheme/host/path/query and MIME data", symbol: "link", good: true)
                    statusCard("Bundle management", detail: "containsKey, remove, clear, size and typed defaults", symbol: "shippingbox", good: true)
                    statusCard("Intent filter semantics", detail: "Data/type exclusivity, categories and filterEquals", symbol: "line.3.horizontal.decrease.circle", good: true)
                    statusCard("In-process Activity backstack", detail: "Runtime dispatch + Back restoration wired · guest Context.startActivity JNI routing still pending", symbol: "arrow.triangle.2.circlepath", good: false)
                    statusCard("ART / Android framework", detail: "Partial implementation · not full compatibility", symbol: "cpu", good: false)
                    statusCard("Native execution pipeline", detail: "AArch64 PT_LOAD memory mapping, topological initializers, and JNI_OnLoad integration", symbol: "memorychip", good: true)
                    statusCard("Native libraries and graphics", detail: "GLES2/EGL and Cocos2d-x render loop integration in progress", symbol: "square.stack.3d.up", good: false)
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
                .font(.system(size: 16, weight: .semibold))
                .foregroundStyle(good ? accent : muted)
                .frame(width: 38, height: 38)
                .background((good ? accent : muted).opacity(0.11), in: RoundedRectangle(cornerRadius: 11))
            VStack(alignment: .leading, spacing: 4) {
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.primary)
                Text(detail)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(muted)
                    .fixedSize(horizontal: false, vertical: true)
                    .lineSpacing(1)
            }
            Spacer(minLength: 5)
            Text(good ? "READY" : "IN PROGRESS")
                .font(.system(size: 8, weight: .heavy))
                .tracking(0.55)
                .foregroundStyle(good ? accent : muted)
                .padding(.horizontal, 8)
                .padding(.vertical, 6)
                .background((good ? accent : muted).opacity(0.10), in: Capsule())
        }
        .padding(.horizontal, 13)
        .padding(.vertical, 12)
        .background(panel, in: RoundedRectangle(cornerRadius: 15))
        .overlay(RoundedRectangle(cornerRadius: 15).stroke(line, lineWidth: 1))
        .accessibilityElement(children: .ignore)
        .accessibilityLabel("\(title), \(good ? "Ready" : "In progress"). \(detail)")
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
        .contextMenu {
            Button(role: .destructive) {
                pendingDelete = item
            } label: {
                Label("Delete App", systemImage: "trash")
            }
        }
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

private struct StrideAccentOption: Identifiable {
    let name: String
    let hex: String
    var id: String { hex }
}

private enum StrideAccentPalette {
    static let colors: [StrideAccentOption] = [
        .init(name: "Amber", hex: "E8AB64"),
        .init(name: "Lime", hex: "C5F477"),
        .init(name: "Violet", hex: "B9A2FF"),
        .init(name: "Blue", hex: "668CFF"),
        .init(name: "Cyan", hex: "63D9E8"),
        .init(name: "Rose", hex: "F28EAD"),
        .init(name: "Red", hex: "F07878"),
        .init(name: "White", hex: "E8EAF0")
    ]
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
                Text("Tap to open")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(muted)
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
        VStack(spacing: 8) {
            APKIconView(data: item.iconData, accent: accent, size: 88)
            Text(item.displayName)
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .foregroundStyle(.white)
                .lineLimit(2)
                .multilineTextAlignment(.center)
                .frame(maxWidth: .infinity)
        }
        .padding(.vertical, 5)
        .frame(maxWidth: .infinity)
        .contentShape(Rectangle())
    }
}

private struct AboutStrideView: View {
    @Environment(\.dismiss) private var dismiss
    let accent: Color
    let panel: Color
    let muted: Color

    var body: some View {
        ZStack {
            Color.black.ignoresSafeArea()
            VStack(alignment: .leading, spacing: 20) {
                HStack {
                    Text("ABOUT STRIDE").font(.system(size: 11, weight: .heavy)).tracking(1.4).foregroundStyle(muted)
                    Spacer()
                    Button("Done") { dismiss() }.font(.system(size: 13, weight: .bold)).foregroundStyle(accent)
                }
                Spacer()
                StrideCoreMark(size: 82, accent: accent)
                Text("Stride").font(.system(size: 34, weight: .black, design: .rounded)).foregroundStyle(.white)
                Text("An experimental Android compatibility layer for iOS, focused on inspecting and executing Android app packages through native runtime components.")
                    .font(.system(size: 14, weight: .medium)).foregroundStyle(muted).fixedSize(horizontal: false, vertical: true)
                VStack(alignment: .leading, spacing: 12) {
                    aboutRow("Execution model", "Native iOS runtime components")
                    aboutRow("Package formats", "APK · XAPK · APKS · APKM")
                    aboutRow("Compatibility", "Experimental; app support requires real-device verification")
                }
                .padding(16).frame(maxWidth: .infinity, alignment: .leading)
                .background(panel, in: RoundedRectangle(cornerRadius: 18))
                Spacer()
                Text("STRIDE · EXPERIMENTAL BUILD").font(.system(size: 9, weight: .bold)).tracking(1.1)
                    .foregroundStyle(muted.opacity(0.75)).frame(maxWidth: .infinity)
            }
            .padding(24)
        }
        .preferredColorScheme(.dark)
    }

    private func aboutRow(_ title: String, _ value: String) -> some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(title.uppercased()).font(.system(size: 8, weight: .heavy)).tracking(1).foregroundStyle(muted)
            Text(value).font(.system(size: 12, weight: .semibold)).foregroundStyle(.white)
        }
    }
}

private struct APKDetailSheet: View {
    @State private var showingLogShare = false
    @State private var showingDiagnostics = false
    @State private var showingExecutionScreen = false
    @State private var isExecutionChromeHidden = false
    @State private var launchTask: Task<Void, Never>?
    @State private var frameScheduler: GuestFrameScheduler?
    @State private var guestFramebufferData: Data?
    @State private var guestFramebufferWidth = 0
    @State private var guestFramebufferHeight = 0
    @State private var guestTouchActive = false
    @State private var touchResponseMilliseconds: Double?
    @State private var liveGuestFPS: Float?
    @State private var lastGuestTouchDiagnostic = ""
    @State private var showingGuestKeyboard = false
    @State private var guestInputText = ""
    @FocusState private var guestKeyboardFocused: Bool
    @Environment(\.dismiss) private var dismiss
    @ObservedObject private var diagnostics = StrideDiagnostics.shared
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

    private var packageSizeLabel: String {
        let manager = FileManager.default
        let root = item.packageDirectoryURL
        var total: Int64 = 0
        var directory: ObjCBool = false
        if manager.fileExists(atPath: root.path, isDirectory: &directory), directory.boolValue,
           let iterator = manager.enumerator(at: root, includingPropertiesForKeys: [.isRegularFileKey, .fileSizeKey], options: [.skipsHiddenFiles]) {
            for case let url as URL in iterator {
                guard let values = try? url.resourceValues(forKeys: [.isRegularFileKey, .fileSizeKey]), values.isRegularFile == true else { continue }
                total += Int64(values.fileSize ?? 0)
            }
        } else {
            total = Int64((try? root.resourceValues(forKeys: [.fileSizeKey]).fileSize) ?? 0)
        }
        return total > 0 ? ByteCountFormatter.string(fromByteCount: total, countStyle: .file) : "Unknown size"
    }

    private var lastPlayedLabel: String {
        let key = "stride.lastPlayed.\(item.packageDirectoryURL.standardizedFileURL.path)"
        guard let date = UserDefaults.standard.object(forKey: key) as? Date else { return "Never played" }
        return date.formatted(date: .abbreviated, time: .shortened)
    }

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
                            HStack(spacing: 7) {
                                Label(packageSizeLabel, systemImage: "internaldrive")
                                Label(lastPlayedLabel, systemImage: "clock")
                            }
                            .font(.system(size: 9, weight: .medium))
                            .foregroundStyle(muted)
                            .lineLimit(1)
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

                    DisclosureGroup(isExpanded: $showingDiagnostics) {
                        VStack(alignment: .leading, spacing: 8) {
                            metricRow("Captured events", value: "\(diagnostics.entries.count)")
                            metricRow("Errors", value: "\(diagnostics.issueCount)")
                            metricRow("Warnings", value: "\(diagnostics.entries.filter { $0.severity == .warning }.count)")
                            Text("Export the Complete Diagnostic Report after a launch attempt for the full captured evidence.")
                                .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                                .fixedSize(horizontal: false, vertical: true)
                        }
                        .padding(.top, 8)
                    } label: {
                        Label("Diagnostics", systemImage: "waveform.path.ecg")
                            .font(.system(size: 13, weight: .bold)).foregroundStyle(.white)
                    }
                    .tint(accent).padding(15)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))
                    .overlay(RoundedRectangle(cornerRadius: 16).stroke(Color.white.opacity(0.075), lineWidth: 1))

                    Button {
                        showingExecutionScreen = true
                        launchTask = Task {
                            isExecuting = true
                            executionError = nil
                            frameScheduler?.stop()
                            frameScheduler = nil
                            guestTouchActive = false
                            touchResponseMilliseconds = nil
                            liveGuestFPS = nil
                            lastGuestTouchDiagnostic = ""
                            executionResult = nil
                            sessionLogLines = []
                            diagnostics.beginSession(apkName: item.displayName)
                            logExportURL = diagnostics.currentSessionLogURL
                            appendSessionLog("[SESSION] Force execution requested for \(item.displayName)")
                            appendSessionLog("[SESSION] Started: \(Date().formatted(date: .abbreviated, time: .standard))")
                            do {
                                let backend = StrideArtExecutionBackend()
                                let res = try await backend.launch(packageAt: item.fileURL) { line in
                                    Task { @MainActor in appendSessionLog(line) }
                                }
                                guard !Task.isCancelled else {
                                    res.session.close()
                                    return
                                }
                                executionResult = res
                                UserDefaults.standard.set(Date(), forKey: "stride.lastPlayed.\(item.packageDirectoryURL.standardizedFileURL.path)")
                                // Seed the guest surface before accepting the first gesture. The
                                // touch mapper needs real framebuffer dimensions; waiting for the
                                // first CADisplayLink tick can silently drop an immediate tap.
                                if let initialFrame = res.session.copyFramebuffer() {
                                    guestFramebufferData = initialFrame.data
                                    guestFramebufferWidth = initialFrame.width
                                    guestFramebufferHeight = initialFrame.height
                                    appendSessionLog("[FRAMEBUFFER] Initial guest surface ready (\(initialFrame.width)x\(initialFrame.height))")
                                } else {
                                    appendSessionLog("[WARN] Initial guest framebuffer is not ready; input mapping will wait for the first guest surface")
                                }
                                appendSessionLog("[SESSION] APK screenOrientation=\(res.screenOrientation) recorded; iOS auto-rotation remains enabled")
                                lastRunLabel = "Runtime retained · \(Date().formatted(date: .abbreviated, time: .shortened))"
                                let scheduler = GuestFrameScheduler { _ in
                                    let pump = res.session.pumpGuestEvents()
                                    let guestFrames = res.session.guestFrameStats()
                                    if let fps = guestFrames.fps {
                                        liveGuestFPS = fps
                                    }
                                    if pump.status == 0 {
                                        if pump.delivered > 0 {
                                            appendSessionLog("[INPUT] Delivered \(pump.delivered) Android touch event(s) to the guest Activity")
                                            if let snapshot = res.session.diagnosticsSnapshot() {
                                                let callbackLines = snapshot.components(separatedBy: .newlines).filter {
                                                    $0.contains("ACTIVITY_TOUCH_CALLBACK_EXECUTED") ||
                                                    $0.contains("GUEST_TOUCH_DISPATCH_FAILED")
                                                }
                                                if let latest = callbackLines.last, latest != lastGuestTouchDiagnostic {
                                                    lastGuestTouchDiagnostic = latest
                                                    appendSessionLog("[GUEST] \(latest)")
                                                }
                                            }
                                        }
                                        if let frame = res.session.copyFramebuffer() {
                                            guestFramebufferData = frame.data
                                            guestFramebufferWidth = frame.width
                                            guestFramebufferHeight = frame.height
                                            // Rust timestamps each queued event and reports the
                                            // actual guest callback plus guest-render processing time.
                                            if let measured = pump.touchMilliseconds {
                                                touchResponseMilliseconds = Double(measured)
                                            }
                                        }
                                    } else {
                                        appendSessionLog("[ERROR] Guest input pump failed with status \(pump.status)")
                                        frameScheduler?.stop()
                                    }
                                }
                                frameScheduler = scheduler
                                scheduler.start()
                                appendSessionLog("[SESSION] Host display-link polls guest input and copies the framebuffer; FPS counts successful guest GL swaps only")
                                appendSessionLog("[SESSION] Tap Close to return to Stride and release runtime resources")
                            } catch {
                                guard !Task.isCancelled else { return }
                                executionError = error.localizedDescription
                                lastRunLabel = "Failed · \(Date().formatted(date: .abbreviated, time: .shortened))"
                                appendSessionLog("[SESSION] FAILED: \(error.localizedDescription)")
                                diagnostics.finishSession(status: "failed: \(error.localizedDescription)")
                            }
                            if !Task.isCancelled { isExecuting = false }
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
                        HStack {
                            Text("Session Logs").font(.system(size: 11, weight: .bold)).foregroundStyle(.white)
                            Spacer()
                            Button { copySessionLog() } label: {
                                Label("COPY LOG", systemImage: "doc.on.doc")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(accent)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Copy complete session log")
                            Button { exportSessionLog() } label: {
                                Label("EXPORT", systemImage: "square.and.arrow.up")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(accent)
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel("Export complete session log as text")
                        }
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
                        Text("One file includes APK/split fingerprints, iPhone and iOS build details, runtime timeline, missing APIs, launch-stage evidence and the full raw log.")
                            .font(.system(size: 10, weight: .medium))
                            .foregroundStyle(muted)
                            .fixedSize(horizontal: false, vertical: true)
                        Button {
                            let runtimeStatus = executionError.map { "Failed: \($0)" } ??
                                executionResult.map { "Status=\($0.statusCode), lifecycle=\($0.lifecycleState)" } ??
                                "Session captured; final runtime state unavailable"
                            let runtimeMetrics = executionResult.map {
                                "classes=\($0.totalClassesLoaded), instructions=\($0.instructionsExecuted), missingAPIs=\($0.missingApisCount), averageFPS=\($0.averageFps), heapBytes=\($0.heapAllocatedBytes)"
                            } ?? "No launch metrics returned"
                            let liveRuntimeSnapshot = executionResult?.session.diagnosticsSnapshot() ??
                                "Live Rust runtime snapshot unavailable (runtime was not retained or export failed)"
                            let completeSessionLog = sessionLogLines.joined(separator: "\n") +
                                "\n\n--- FINAL LIVE RUST DIAGNOSTICS SNAPSHOT ---\n" + liveRuntimeSnapshot
                            let reportURL = diagnostics.exportSessionReport(
                                apkName: item.displayName,
                                apkURL: item.fileURL,
                                status: runtimeStatus,
                                rawLog: completeSessionLog,
                                inspection: item.inspection,
                                packageFormat: String(describing: item.packageFormat),
                                splitCount: item.splitAPKURLs.count,
                                runtimeMetrics: runtimeMetrics,
                                splitAPKURLs: item.splitAPKURLs
                            )
                            if let reportURL {
                                logExportURL = reportURL
                                showingLogShare = true
                            } else {
                                appendSessionLog("[ERROR] One-shot diagnostic report could not be generated")
                            }
                        } label: {
                            Label("Export Complete Diagnostic Report (.txt)", systemImage: "doc.text.magnifyingglass")
                                .font(.system(size: 11, weight: .semibold)).foregroundStyle(accent)
                        }
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(panel, in: RoundedRectangle(cornerRadius: 16))

                    VStack(alignment: .leading, spacing: 10) {
                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text("Unified Diagnostics").font(.system(size: 13, weight: .bold)).foregroundStyle(.white)
                                Text("\(diagnostics.entries.count) retained events · \(diagnostics.issueCount) errors")
                                    .font(.system(size: 10, weight: .medium)).foregroundStyle(muted)
                                HStack(spacing: 9) {
                                    Label("\(diagnostics.entries.filter { $0.severity == .fatal }.count) fatal", systemImage: "exclamationmark.octagon.fill")
                                        .foregroundStyle(.red)
                                    Label("\(diagnostics.entries.filter { $0.severity == .warning }.count) warnings", systemImage: "exclamationmark.triangle.fill")
                                        .foregroundStyle(.orange)
                                }
                                .font(.system(size: 9, weight: .semibold, design: .rounded))
                            }
                            Spacer()
                            Button {
                                UIPasteboard.general.string = diagnostics.entries.map(\.line).joined(separator: "\n")
                            } label: {
                                Label("COPY", systemImage: "doc.on.doc")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(accent)
                            }
                            Button {
                                let fatalCount = diagnostics.entries.filter { $0.severity == .fatal }.count
                                let errorCount = diagnostics.entries.filter { $0.severity == .error }.count
                                let warningCount = diagnostics.entries.filter { $0.severity == .warning }.count
                                let recent = diagnostics.entries.suffix(12).map(\.line).joined(separator: "\n")
                                let summary = [
                                    "STRIDE DIAGNOSTICS SUMMARY",
                                    "Events: \(diagnostics.entries.count)",
                                    "Fatal: \(fatalCount) · Errors: \(errorCount) · Warnings: \(warningCount)",
                                    "",
                                    "RECENT EVENTS",
                                    recent
                                ].joined(separator: "\n")
                                UIPasteboard.general.string = summary
                            } label: {
                                Label("SUMMARY", systemImage: "doc.text.magnifyingglass")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(accent)
                            }
                            Button {
                                exportSessionLog()
                            } label: {
                                Label("EXPORT", systemImage: "square.and.arrow.up")
                                    .font(.system(size: 9, weight: .bold))
                                    .foregroundStyle(accent)
                            }
                        }
                        ScrollView {
                            LazyVStack(alignment: .leading, spacing: 6) {
                                ForEach(Array(diagnostics.entries.suffix(35))) { entry in
                                    Text(entry.line)
                                        .font(.system(size: 9, weight: .regular, design: .monospaced))
                                        .foregroundStyle(entry.severity == .fatal || entry.severity == .error ? Color.red : (entry.severity == .warning ? Color.orange : muted))
                                        .frame(maxWidth: .infinity, alignment: .leading)
                                        .textSelection(.enabled)
                                }
                            }
                        }
                        .frame(maxHeight: 210)
                        .padding(10)
                        .background(canvas, in: RoundedRectangle(cornerRadius: 11))
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
        .background(canvas.ignoresSafeArea())
        .fullScreenCover(isPresented: $showingExecutionScreen) {
            executionScreen
                .statusBarHidden(true)
                .interactiveDismissDisabled(true)
        }
        .onAppear(perform: loadPreviousSession)
        .onDisappear {
            launchTask?.cancel()
            launchTask = nil
            frameScheduler?.stop()
            frameScheduler = nil
            touchResponseMilliseconds = nil
            liveGuestFPS = nil
            StrideOrientationController.shared.reset()
            executionResult?.session.close()
            diagnostics.finishSession(status: isExecuting
                ? "view dismissed while launch was active"
                : (executionError.map { "view dismissed after launch failure: \($0)" } ?? "runtime released by user"))
        }
    }

    private var executionScreen: some View {
        ZStack {
            Color.black.ignoresSafeArea()

            if !isExecutionChromeHidden {
                VStack(spacing: 0) {
                    HStack(spacing: 10) {
                        Button {
                            closeExecutionSession()
                        } label: {
                            HStack(spacing: 5) {
                                Image(systemName: "xmark")
                                    .font(.system(size: 11, weight: .bold))
                                Text("Close")
                                    .font(.system(size: 12, weight: .semibold))
                            }
                            .foregroundStyle(.white)
                        }
                        .buttonStyle(.plain)

                        Circle()
                            .fill(executionStatusColor)
                            .frame(width: 7, height: 7)

                        Text(executionStatusText)
                            .font(.system(size: 10, weight: .semibold))
                            .foregroundStyle(.white.opacity(0.88))
                            .lineLimit(1)
                            .minimumScaleFactor(0.75)

                        Spacer(minLength: 6)

                        executionMetric("FPS", value: liveFPSLabel)
                        executionMetric("TOUCH MS", value: touchLatencyLabel)

                        Button {
                            guestInputText = ""
                            showingGuestKeyboard = true
                            guestKeyboardFocused = true
                        } label: {
                            Image(systemName: "keyboard")
                                .font(.system(size: 14, weight: .medium))
                                .foregroundStyle(.white.opacity(0.85))
                                .frame(width: 30, height: 30)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Open guest text keyboard")

                        Button {
                            withAnimation(.easeInOut(duration: 0.18)) {
                                isExecutionChromeHidden = true
                            }
                        } label: {
                            Image(systemName: "eye.slash")
                                .font(.system(size: 14, weight: .medium))
                                .foregroundStyle(.white.opacity(0.85))
                                .frame(width: 30, height: 30)
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel("Hide execution status bar")
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 11)
                    .background(Color(white: 0.075))

                    guestFramebufferSurface.overlay { guestTouchSurface }
                }
                .transition(.move(edge: .top).combined(with: .opacity))
            } else {
                guestFramebufferSurface
                    .overlay { guestTouchSurface }
                    .ignoresSafeArea()
                    .transition(.opacity)
            }
        }
        .overlay(alignment: .bottom) {
            if showingGuestKeyboard {
                HStack(spacing: 8) {
                    TextField("Type into Android app", text: $guestInputText)
                        .textFieldStyle(.roundedBorder)
                        .focused($guestKeyboardFocused)
                        .submitLabel(.done)
                        .onSubmit { showingGuestKeyboard = false; guestKeyboardFocused = false }
                    Button("Done") { showingGuestKeyboard = false; guestKeyboardFocused = false }
                        .font(.system(size: 13, weight: .semibold))
                }
                .padding(12)
                .background(Color(white: 0.08))
                .onChange(of: guestInputText) { newValue in
                    let status = executionResult?.session.setFocusedGuestText(newValue) ?? 1
                    if status == 4 {
                        appendSessionLog("[INPUT] Keyboard text ignored: tap an Android EditText first")
                    } else if status != 0 {
                        appendSessionLog("[INPUT] Guest text injection failed (status=\(status))")
                    }
                }
            }
        }
        .contentShape(Rectangle())
        // Detect a simultaneous three-finger chord, not three sequential taps.
        // The recognizer is non-blocking and coexists with guest touch input.
        .background {
            ThreeFingerTouchToggle {
                withAnimation(.easeInOut(duration: 0.18)) {
                    isExecutionChromeHidden.toggle()
                }
            }
            .frame(width: 0, height: 0)
            .allowsHitTesting(false)
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showingLogShare) {
            if let logExportURL {
                ActivityViewController(activityItems: [logExportURL])
            }
        }
    }

    private func closeExecutionSession() {
        launchTask?.cancel()
        launchTask = nil
        frameScheduler?.stop()
        frameScheduler = nil
        guestTouchActive = false
        touchResponseMilliseconds = nil
        liveGuestFPS = nil
        showingGuestKeyboard = false
        guestKeyboardFocused = false
        guestFramebufferData = nil
        guestFramebufferWidth = 0
        guestFramebufferHeight = 0
        showingExecutionScreen = false
        isExecutionChromeHidden = false
        executionResult?.session.close()
        executionResult = nil
        StrideOrientationController.shared.reset()
        diagnostics.finishSession(status: "runtime released by user")
        appendSessionLog("[SESSION] Runtime closed safely by user")
        dismiss()
    }

    private var executionStatusText: String {
        if isExecuting { return "Starting…" }
        if executionError != nil { return "Could not start — see the log" }
        if executionResult != nil {
            guard let fps = liveGuestFPS else { return "Activity resumed · waiting for guest frames" }
            return fps > 0 ? "Guest rendering active" : "Guest renderer idle"
        }
        return "Ready"
    }

    private var guestFramebufferSurface: some View {
        Group {
            if let data=guestFramebufferData,guestFramebufferWidth>0,guestFramebufferHeight>0,
               let provider=CGDataProvider(data:data as CFData),
               let cg=CGImage(width:guestFramebufferWidth,height:guestFramebufferHeight,bitsPerComponent:8,bitsPerPixel:32,
                bytesPerRow:guestFramebufferWidth*4,space:CGColorSpaceCreateDeviceRGB(),
                bitmapInfo:CGBitmapInfo(rawValue:CGBitmapInfo.byteOrder32Little.rawValue|CGImageAlphaInfo.premultipliedFirst.rawValue),
                provider:provider,decode:nil,shouldInterpolate:false,intent:.defaultIntent) {
                Image(uiImage:UIImage(cgImage:cg)).resizable().interpolation(.none).scaledToFit().frame(maxWidth:.infinity,maxHeight:.infinity).background(Color.black)
            } else { Color.black }
        }.accessibilityLabel("Android guest framebuffer")
    }

    private var guestTouchSurface: some View {
        GeometryReader { geometry in
            Color.clear
                .contentShape(Rectangle())
                .simultaneousGesture(
                    DragGesture(minimumDistance: 0, coordinateSpace: .local)
                        .onChanged { value in
                            guard let point = guestSurfacePoint(
                                value.location,
                                container: geometry.size,
                                surfaceWidth: guestFramebufferWidth,
                                surfaceHeight: guestFramebufferHeight
                            ) else { return }
                            let action: Int32 = guestTouchActive ? 2 : 0
                            guestTouchActive = true
                            measureGuestTouchDispatch(action: action, point: point)
                        }
                        .onEnded { value in
                            if let point = guestSurfacePoint(
                                value.location,
                                container: geometry.size,
                                surfaceWidth: guestFramebufferWidth,
                                surfaceHeight: guestFramebufferHeight
                            ) {
                                measureGuestTouchDispatch(action: 1, point: point)
                            }
                            guestTouchActive = false
                        }
                )
                .frame(width: geometry.size.width, height: geometry.size.height)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .accessibilityLabel("Android app touch surface")
    }

    /// Maps iOS points through the same aspect-fit rectangle used by the
    /// framebuffer Image, returning Android guest-surface pixel coordinates.
    private func guestSurfacePoint(
        _ point: CGPoint,
        container: CGSize,
        surfaceWidth: Int,
        surfaceHeight: Int
    ) -> CGPoint? {
        guard container.width > 0, container.height > 0,
              surfaceWidth > 0, surfaceHeight > 0 else { return nil }
        let width = CGFloat(surfaceWidth)
        let height = CGFloat(surfaceHeight)
        let scale = min(container.width / width, container.height / height)
        let renderedWidth = width * scale
        let renderedHeight = height * scale
        let originX = (container.width - renderedWidth) * 0.5
        let originY = (container.height - renderedHeight) * 0.5
        let x = point.x - originX
        let y = point.y - originY
        guard x >= 0, y >= 0, x <= renderedWidth, y <= renderedHeight else { return nil }
        return CGPoint(x: x / renderedWidth * width, y: y / renderedHeight * height)
    }

    private var executionStatusColor: Color {
        if executionError != nil { return .red }
        if isExecuting || executionResult != nil { return .orange }
        return .green
    }

    // FPS is sourced only from successful guest OpenGL buffer swaps.
    // Host CADisplayLink ticks and framebuffer polling never count as guest frames.
    private var liveFPSLabel: String {
        guard let fps = liveGuestFPS else { return "--" }
        return String(format: "%.0f", fps)
    }

    // Per-input sample from Rust: queue enqueue -> guest callback processing -> guest surface redraw.
    // No touch or no guest response means no fabricated value is displayed.
    private var touchLatencyLabel: String {
        guard let value = touchResponseMilliseconds else { return "--" }
        return String(format: "%.2f", value)
    }

    private func measureGuestTouchDispatch(action: Int32, point: CGPoint) {
        guard let session = executionResult?.session else { return }
        let accepted = session.dispatchTouch(action: action, x: Float(point.x), y: Float(point.y))
        if action != 2 {
            appendSessionLog(accepted
                ? "[INPUT] Host touch queued: action=\(action) x=\(Int(point.x)) y=\(Int(point.y))"
                : "[ERROR] Host touch could not be queued: action=\(action)")
        }
    }

    private func executionMetric(_ title: String, value: String) -> some View {
        VStack(alignment: .trailing, spacing: 2) {
            Text(value)
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(.white)
                .lineLimit(1)
            Text(title)
                .font(.system(size: 7, weight: .heavy))
                .foregroundStyle(.white.opacity(0.48))
        }
        .frame(minWidth: 35, alignment: .trailing)
        .accessibilityElement(children: .combine)
    }

    private func logFileURL() -> URL {
        diagnostics.latestSessionLogURL(apkName: item.displayName) ?? diagnostics.logFileURL
    }

    private func sessionLogExportText() -> String {
        diagnostics.flush()
        let inMemoryLog = sessionLogLines.joined(separator: "\n")
        let persistedLog = diagnostics.currentSessionLogURL
            .flatMap { try? String(contentsOf: $0, encoding: .utf8) } ?? ""
        let sessionText = inMemoryLog.isEmpty ? persistedLog : inMemoryLog
        let structuredText = diagnostics.entries.map(\.line).joined(separator: "\n")
        return [
            "STRIDE SESSION LOG",
            "APK: \(item.displayName)",
            "Exported: \(Date().formatted(date: .abbreviated, time: .standard))",
            "",
            "SESSION EVENTS",
            sessionText.isEmpty ? "(No session events were captured.)" : sessionText,
            "",
            "UNIFIED DIAGNOSTICS",
            structuredText.isEmpty ? "(No structured diagnostic events were captured.)" : structuredText
        ].joined(separator: "\n") + "\n"
    }

    private func copySessionLog() {
        UIPasteboard.general.string = sessionLogExportText()
    }

    private func exportSessionLog() {
        let report = sessionLogExportText()
        let safeName = item.displayName.replacingOccurrences(
            of: "[^A-Za-z0-9._-]", with: "_", options: .regularExpression
        )
        let timestamp = ISO8601DateFormatter().string(from: Date())
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: ".", with: "-")
        let url = FileManager.default.temporaryDirectory
            .appendingPathComponent("Stride-\(safeName)-\(timestamp)-Session-Log.txt")
        do {
            try report.write(to: url, atomically: true, encoding: .utf8)
            let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
            guard ((attributes[.size] as? NSNumber)?.intValue ?? 0) > 0 else {
                throw CocoaError(.fileWriteUnknown)
            }
            logExportURL = url
            showingLogShare = true
        } catch {
            appendSessionLog("[ERROR] Session log export failed: \(error.localizedDescription)")
        }
    }

    private func appendSessionLog(_ message: String) {
        let stamp = ISO8601DateFormatter().string(from: Date())
        sessionLogLines.append("[\(stamp)] \(message)")
        if sessionLogLines.count > 2_000 {
            sessionLogLines.removeFirst(sessionLogLines.count - 2_000)
        }
        let upper = message.uppercased()
        let isNativeCrash = upper.contains("FATAL SIGNAL") || upper.contains("SIGSEGV") ||
            upper.contains("SIGABRT") || upper.contains("SIGBUS") || upper.contains("SIGILL") || upper.contains("SIGFPE")
        let isSyscall = upper.contains("SYSTEM CALL") && (upper.contains("ENOSYS") || upper.contains("NOT PROVIDED"))
        let isUnresolvedImport = upper.contains("UNRESOLVED IMPORT")
        let isNetworkTelemetry = upper.contains("FIREBASELOGGING") ||
            (upper.contains("CONNECTEXCEPTION") && upper.contains(":443"))
        let isRelocation = upper.contains("RELOCATION") && (upper.contains("FAILED") || upper.contains("UNRESOLVED"))
        let isInterpreterFailure = upper.contains("UNSUPPORTED OPCODE") || upper.contains("INTERPRETER FAULT")
        let severity: StrideDiagnostics.Severity =
            isNativeCrash || upper.contains("[FATAL]") || upper.contains("[CRASH]") ? .fatal :
            (upper.contains("[ERROR]") || upper.contains("FAILED") ? .error :
            (upper.contains("[WARN]") || isSyscall || isUnresolvedImport || isNetworkTelemetry ? .warning : .info))
        let code = isNativeCrash ? "NATIVE_CRASH" :
            (isSyscall ? "UNSUPPORTED_SYSCALL" :
            (isUnresolvedImport ? "UNRESOLVED_NATIVE_IMPORT" :
            (isNetworkTelemetry ? "NETWORK_TELEMETRY_FAILURE" :
            (isRelocation ? "RELOCATION_FAILURE" :
            (isInterpreterFailure ? "RUNTIME_INTERPRETER_FAILURE" : "RUNTIME_EVENT")))))
        let stage: String
        switch true {
        case isNativeCrash: stage = "NATIVE_CRASH"
        case isNetworkTelemetry: stage = "NETWORK"
        case message.hasPrefix("[SESSION]"): stage = "SESSION"
        case upper.contains("MANIFEST"): stage = "MANIFEST"
        case upper.contains("DEX") || isInterpreterFailure: stage = "DEX"
        case upper.contains("ELF") || upper.contains("NATIVE LIBRARY"): stage = "ELF"
        case upper.contains("JNI"): stage = "JNI"
        case upper.contains("TOUCH") || upper.contains("INPUT"): stage = "INPUT"
        case upper.contains("FRAME") || upper.contains("RENDER") || upper.contains("CANVAS"): stage = "GRAPHICS"
        case upper.contains("PACKAGEINFO") || upper.contains("PACKAGEMANAGER") || upper.contains("APPLICATIONINFO"): stage = "PACKAGEMANAGER"
        case upper.contains("ACTIVITY") || upper.contains("LIFECYCLE"): stage = "LIFECYCLE"
        case upper.contains("ARCHIVE") || upper.contains("APK"): stage = "ARCHIVE"
        case upper.contains("CLASS") || upper.contains("METHOD"): stage = "DEX_LINKER"
        case upper.contains("RUNTIME"): stage = "ART_RUNTIME"
        default: stage = "EXECUTION"
        }
        let recovery: String? = isNativeCrash ? "Preserve signal, fault address, registers, module offsets and backtrace; symbolize against the exact library build." :
            (isSyscall ? "Check syscall compatibility and implement only understood guest semantics." :
            (isUnresolvedImport ? "Resolve against compatible Bionic/libc behavior; do not silently bind unsafe stubs." : nil))
        diagnostics.record(severity, stage: stage, code: code,
                           message.replacingOccurrences(of: "[RUNTIME] ", with: ""),
                           recovery: recovery)
        logExportURL = diagnostics.currentSessionLogURL ?? logFileURL()
    }

    private func loadPreviousSession() {
        diagnostics.flush()
        let url = logFileURL()
        guard let content = try? String(contentsOf: url, encoding: .utf8), !content.isEmpty else { return }
        sessionLogLines = content.components(separatedBy: .newlines).filter { !$0.isEmpty }
        logExportURL = url
        if let last = sessionLogLines.last { lastRunLabel = "Saved session · \(last)" }
    }

    private func metricRow(_ title: String, value: String) -> some View {
        HStack {
            Text(title)
            Spacer()
            Text(value).monospacedDigit().foregroundStyle(muted)
        }
        .font(.system(size: 11, weight: .medium))
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
        .clipShape(RoundedRectangle(cornerRadius: size * 0.25, style: .continuous))
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





private struct ThreeFingerTouchToggle: UIViewRepresentable {
    let onToggle: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(onToggle: onToggle)
    }

    func makeUIView(context: Context) -> GestureAnchorView {
        let view = GestureAnchorView()
        view.onWindowChange = { [weak coordinator = context.coordinator] window in
            coordinator?.install(on: window)
        }
        return view
    }

    func updateUIView(_ uiView: GestureAnchorView, context: Context) {
        context.coordinator.onToggle = onToggle
        context.coordinator.install(on: uiView.window)
    }

    static func dismantleUIView(_ uiView: GestureAnchorView, coordinator: Coordinator) {
        coordinator.remove()
    }

    final class Coordinator {
        var onToggle: () -> Void
        private let recognizer: ThreeFingerChordRecognizer

        init(onToggle: @escaping () -> Void) {
            self.onToggle = onToggle
            self.recognizer = ThreeFingerChordRecognizer()
            self.recognizer.onRecognized = { [weak self] in self?.onToggle() }
        }

        func install(on window: UIWindow?) {
            guard let window else { return }
            if recognizer.view !== window {
                recognizer.view?.removeGestureRecognizer(recognizer)
                window.addGestureRecognizer(recognizer)
            }
        }

        func remove() {
            recognizer.view?.removeGestureRecognizer(recognizer)
        }
    }
}

private final class GestureAnchorView: UIView {
    var onWindowChange: ((UIWindow?) -> Void)?

    override func didMoveToWindow() {
        super.didMoveToWindow()
        onWindowChange?(window)
    }
}

private final class ThreeFingerChordRecognizer: UIGestureRecognizer, UIGestureRecognizerDelegate {
    var onRecognized: (() -> Void)?
    private var touchStartTimes: [ObjectIdentifier: TimeInterval] = [:]
    private var didFire = false
    private let maximumFingerArrivalSpread: TimeInterval = 0.22

    override init(target: Any?, action: Selector?) {
        super.init(target: target, action: action)
        delegate = self
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        guard !didFire else { return }
        for touch in touches {
            touchStartTimes[ObjectIdentifier(touch)] = touch.timestamp
        }

        guard touchStartTimes.count >= 3 else { return }
        let starts = Array(touchStartTimes.values)
        guard let earliest = starts.min(), let latest = starts.max(),
              latest - earliest <= maximumFingerArrivalSpread else {
            state = .failed
            return
        }

        didFire = true
        state = .recognized
        onRecognized?()
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        for touch in touches {
            touchStartTimes.removeValue(forKey: ObjectIdentifier(touch))
        }
        if touchStartTimes.isEmpty && state == .possible {
            state = .failed
        }
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        state = .cancelled
    }

    override func reset() {
        super.reset()
        touchStartTimes.removeAll(keepingCapacity: true)
        didFire = false
    }

    func gestureRecognizer(_ gestureRecognizer: UIGestureRecognizer,
                           shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer) -> Bool {
        true
    }

    override func canPrevent(_ preventedGestureRecognizer: UIGestureRecognizer) -> Bool {
        false
    }

    override func canBePrevented(by preventingGestureRecognizer: UIGestureRecognizer) -> Bool {
        false
    }
}
