import SwiftUI
import UIKit
import PhotosUI

/// Independent iPhone-style launcher surface. The original Stride Home remains a separate,
/// selectable experience; this view only presents packages already imported into Stride.
struct StrideIOSHomeScreen: View {
    let packages: [ImportedAPK]
    let accent: Color
    let onSelect: (ImportedAPK) -> Void
    let onQuickLaunch: (ImportedAPK) -> Void
    let onDelete: (ImportedAPK) -> Void
    let onImport: () -> Void
    let onSettings: () -> Void
    let onExitHomeScreen: () -> Void

    @AppStorage("stride.iosHome.wallpaper") private var wallpaper = "aurora"
    @AppStorage("stride.iosHome.wallpaperImagePath") private var customWallpaperPath = ""
    @AppStorage("stride.iosHome.iconAppearance") private var iconAppearance = "automatic"
    @AppStorage("stride.iosHome.largeIcons") private var largeIcons = false
    @State private var showWallpaperGallery = false
    @State private var showPhotoPicker = false
    @State private var showHomeCustomization = false
    @State private var searchText = ""
    @State private var isSearchPresented = false
    @State private var selectedPage = 0
    @State private var isEditingIcons = false
    @State private var isJiggling = false
    @State private var draggedPackagePath: String?
    @AppStorage("stride.iosHome.appOrder") private var storedAppOrder = ""
    @AppStorage("stride.iosHome.pageCount") private var storedPageCount = 1
    @FocusState private var searchFocused: Bool

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 18), count: 4)

    private var orderedPackages: [ImportedAPK] {
        let paths = storedAppOrder.split(separator: "\n").map(String.init)
        let byPath = Dictionary(uniqueKeysWithValues: packages.map { ($0.packageDirectoryURL.path, $0) })
        let saved = paths.compactMap { byPath[$0] }
        let savedPaths = Set(saved.map { $0.packageDirectoryURL.path })
        return saved + packages.filter { !savedPaths.contains($0.packageDirectoryURL.path) }
    }

    private var filteredPackages: [ImportedAPK] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return orderedPackages }
        return orderedPackages.filter { $0.displayName.localizedCaseInsensitiveContains(query) }
    }

    // One page is created initially. Additional pages are user-created; imported
    // apps automatically extend the Home Screen only when the current page fills.
    private var pages: [[ImportedAPK]] {
        let populated = stridePages(filteredPackages, size: 24)
        let requiredPageCount = max(1, max(storedPageCount, populated.count))
        return populated + Array(repeating: [], count: requiredPageCount - populated.count)
    }

    private var dockPackages: [ImportedAPK] {
        Array(orderedPackages.prefix(4))
    }

    private func persistOrder(_ values: [ImportedAPK]) {
        storedAppOrder = values.map { $0.packageDirectoryURL.path }.joined(separator: "\n")
    }

    private func addHomeScreenPage() {
        storedPageCount = max(1, pages.count) + 1
        selectedPage = storedPageCount - 1
    }

    private func movePackage(path: String, to destinationIndex: Int) {
        guard searchText.isEmpty,
              let sourceIndex = orderedPackages.firstIndex(where: { $0.packageDirectoryURL.path == path }) else { return }
        var values = orderedPackages
        let moved = values.remove(at: sourceIndex)
        let safeIndex = min(max(0, destinationIndex), values.count)
        values.insert(moved, at: safeIndex)
        persistOrder(values)
    }

    private func finishIconEditing() {
        isEditingIcons = false
        isJiggling = false
        draggedPackagePath = nil
    }

    private var wallpaperBackground: some View {
        GeometryReader { geometry in
            ZStack {
                if !customWallpaperPath.isEmpty,
                   let image = UIImage(contentsOfFile: customWallpaperPath) {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFill()
                        .frame(width: geometry.size.width, height: geometry.size.height)
                        .clipped()
                } else {
                    LinearGradient(colors: wallpaperPalette(wallpaper), startPoint: .topLeading, endPoint: .bottomTrailing)
                    Circle()
                        .fill(RadialGradient(colors: [wallpaperGlow(wallpaper).opacity(0.88), .clear],
                                             center: .center, startRadius: 1, endRadius: geometry.size.width * 0.72))
                        .frame(width: geometry.size.width * 1.25, height: geometry.size.width * 1.25)
                        .blur(radius: 22)
                        .offset(x: geometry.size.width * 0.35, y: -geometry.size.height * 0.27)
                    Ellipse()
                        .fill(RadialGradient(colors: [wallpaperAccent(wallpaper).opacity(0.72), .clear],
                                             center: .center, startRadius: 1, endRadius: geometry.size.width * 0.65))
                        .frame(width: geometry.size.width * 1.18, height: geometry.size.height * 0.58)
                        .blur(radius: 27)
                        .offset(x: -geometry.size.width * 0.30, y: geometry.size.height * 0.22)
                    if wallpaper == "cosmic" {
                        ForEach(0..<34, id: \.self) { index in
                            Circle()
                                .fill(Color.white.opacity(index.isMultiple(of: 3) ? 0.8 : 0.42))
                                .frame(width: index.isMultiple(of: 4) ? 2.4 : 1.2)
                                .position(
                                    x: CGFloat((index * 73 + 29) % 397) / 397 * geometry.size.width,
                                    y: CGFloat((index * 47 + 13) % 811) / 811 * geometry.size.height
                                )
                        }
                    }
                    if wallpaper == "dunes" {
                        Ellipse().fill(Color(red: 0.95, green: 0.70, blue: 0.45).opacity(0.52))
                            .frame(width: geometry.size.width * 1.6, height: geometry.size.height * 0.35)
                            .blur(radius: 18)
                            .offset(x: geometry.size.width * 0.16, y: geometry.size.height * 0.36)
                    }
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
            .contentShape(Rectangle())
            .onLongPressGesture(minimumDuration: 0.55) {
                isEditingIcons = true
                showWallpaperGallery = true
            }
        }
        .ignoresSafeArea()
    }

    private func wallpaperPalette(_ name: String) -> [Color] {
        switch name {
        case "ocean": return [Color(red: 0.015, green: 0.35, blue: 0.48), Color(red: 0.008, green: 0.055, blue: 0.20)]
        case "sunset": return [Color(red: 0.98, green: 0.42, blue: 0.34), Color(red: 0.28, green: 0.08, blue: 0.34)]
        case "cosmic": return [Color(red: 0.025, green: 0.035, blue: 0.16), Color(red: 0.20, green: 0.08, blue: 0.38)]
        case "dunes": return [Color(red: 0.78, green: 0.52, blue: 0.34), Color(red: 0.24, green: 0.12, blue: 0.20)]
        case "midnight": return [Color(red: 0.015, green: 0.025, blue: 0.075), Color(red: 0.035, green: 0.12, blue: 0.22)]
        case "black": return [.black, .black]
        default: return [Color(red: 0.015, green: 0.23, blue: 0.28), Color(red: 0.12, green: 0.055, blue: 0.32)]
        }
    }

    private func wallpaperGlow(_ name: String) -> Color {
        switch name {
        case "ocean": return Color.cyan
        case "sunset": return Color.orange
        case "cosmic": return Color.purple
        case "dunes": return Color(red: 1, green: 0.76, blue: 0.48)
        case "midnight": return Color.blue
        case "black": return .black
        default: return Color(red: 0.18, green: 0.92, blue: 0.74)
        }
    }

    private func wallpaperAccent(_ name: String) -> Color {
        switch name {
        case "ocean": return Color(red: 0.12, green: 0.66, blue: 0.82)
        case "sunset": return Color.pink
        case "cosmic": return Color(red: 0.55, green: 0.30, blue: 1)
        case "dunes": return Color(red: 0.90, green: 0.42, blue: 0.28)
        case "midnight": return Color(red: 0.10, green: 0.38, blue: 0.62)
        case "black": return .black
        default: return Color(red: 0.32, green: 0.48, blue: 1)
        }
    }

    private func wallpaperPreview(_ name: String) -> some View {
        ZStack {
            LinearGradient(colors: wallpaperPalette(name), startPoint: .topLeading, endPoint: .bottomTrailing)
            Circle().fill(wallpaperGlow(name).opacity(0.8)).frame(width: 105, height: 105).blur(radius: 20).offset(x: 38, y: -32)
            Ellipse().fill(wallpaperAccent(name).opacity(0.7)).frame(width: 130, height: 80).blur(radius: 18).offset(x: -35, y: 42)
            if name == "cosmic" {
                ForEach(0..<12, id: \.self) { i in
                    Circle().fill(.white.opacity(0.8)).frame(width: 1.5).offset(x: CGFloat((i * 19) % 88 - 44), y: CGFloat((i * 31) % 120 - 60))
                }
            }
            VStack {
                HStack {
                    ForEach(0..<4, id: \.self) { _ in
                        RoundedRectangle(cornerRadius: 7).fill(.white.opacity(0.72)).frame(width: 21, height: 21)
                    }
                }
                Spacer()
                Capsule().fill(.white.opacity(0.34)).frame(width: 64, height: 8)
            }
            .padding(13)
        }
        .frame(height: 142)
        .clipShape(RoundedRectangle(cornerRadius: 17, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 17).stroke(.white.opacity(0.22), lineWidth: 0.8))
    }

    private var wallpaperGallery: some View {
        NavigationView {
            ScrollView {
                LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 14) {
                    ForEach(["aurora", "ocean", "sunset", "cosmic", "dunes", "midnight", "black"], id: \.self) { name in
                        Button {
                            wallpaper = name
                            customWallpaperPath = ""
                            showWallpaperGallery = false
                        } label: {
                            VStack(alignment: .leading, spacing: 7) {
                                wallpaperPreview(name)
                                Text(name.capitalized)
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(.primary)
                            }
                        }
                        .buttonStyle(.plain)
                    }
                    Button {
                        showWallpaperGallery = false
                        DispatchQueue.main.asyncAfter(deadline: .now() + 0.35) {
                            showPhotoPicker = true
                        }
                    } label: {
                        VStack(spacing: 10) {
                            Image(systemName: "photo.on.rectangle.angled")
                                .font(.system(size: 30, weight: .light))
                            Text("Choose Your Photo")
                                .font(.system(size: 13, weight: .semibold))
                        }
                        .foregroundStyle(.primary)
                        .frame(maxWidth: .infinity)
                        .frame(height: 142)
                        .background(Color.secondary.opacity(0.10), in: RoundedRectangle(cornerRadius: 17))
                    }
                    .buttonStyle(.plain)
                }
                .padding(16)
            }
            .background(Color(uiColor: .systemGroupedBackground))
            .navigationTitle("Wallpapers")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { showWallpaperGallery = false }
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private var homeCustomizationSheet: some View {
        NavigationView {
            Form {
                Section("App icon appearance") {
                    Picker("Appearance", selection: $iconAppearance) {
                        Text("Automatic").tag("automatic")
                        Text("Dark tint").tag("dark")
                        Text("Color tint").tag("tinted")
                        Text("Glass overlay").tag("clear")
                    }
                    .pickerStyle(.inline)
                }
                Section("App icon size") {
                    Picker("Size", selection: $largeIcons) {
                        Text("Small").tag(false)
                        Text("Large").tag(true)
                    }
                    .pickerStyle(.segmented)
                }
                Section {
                    Button("Change Wallpaper") {
                        showHomeCustomization = false
                        showWallpaperGallery = true
                    }
                }
            }
            .navigationTitle("Customize")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { showHomeCustomization = false }
                }
            }
        }
        .preferredColorScheme(.dark)
    }

    private func saveCustomWallpaper(_ image: UIImage) {
        guard let data = image.jpegData(compressionQuality: 0.92) else { return }
        let directory = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask)[0]
        let destination = directory.appendingPathComponent("StrideHomeWallpaper.jpg")
        do {
            try data.write(to: destination, options: .atomic)
            customWallpaperPath = destination.path
            showPhotoPicker = false
            showWallpaperGallery = false
        } catch {
            // Keep the currently selected wallpaper if the image cannot be saved.
        }
    }

    var body: some View {
        ZStack {
            wallpaperBackground

            VStack(spacing: 0) {
                launcherPages
                    .padding(.top, 4)
                    .frame(maxHeight: .infinity)

                if isSearchPresented {
                    activeSearchBar
                        .padding(.horizontal, 22)
                        .padding(.bottom, 9)
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                } else {
                    searchButton
                        .padding(.bottom, 10)
                        .transition(.opacity)
                }

                if !searchText.isEmpty {
                    Text("\(filteredPackages.count) RESULTS")
                        .font(.system(size: 9, weight: .bold, design: .rounded))
                        .tracking(1.1)
                        .foregroundStyle(.white.opacity(0.68))
                        .padding(.bottom, 8)
                } else {
                    pageControls
                        .padding(.bottom, 9)
                }

                dock
                    .padding(.horizontal, 18)
                    .padding(.vertical, 18)
                    .background {
                        RoundedRectangle(cornerRadius: 31, style: .continuous)
                            .fill(.ultraThinMaterial)
                            .overlay {
                                RoundedRectangle(cornerRadius: 31, style: .continuous)
                                    .fill(LinearGradient(
                                        colors: [Color.white.opacity(0.13), Color.white.opacity(0.035), Color.black.opacity(0.08)],
                                        startPoint: .topLeading, endPoint: .bottomTrailing
                                    ))
                            }
                            .overlay {
                                RoundedRectangle(cornerRadius: 31, style: .continuous)
                                    .stroke(Color.white.opacity(0.22), lineWidth: 0.9)
                            }
                            .shadow(color: .black.opacity(0.24), radius: 18, y: 8)
                    }
                    .padding(.horizontal, 17)
                    .padding(.bottom, 7)
            }
        }
        .overlay(alignment: .topTrailing) {
            // Overlay the return menu in the status-bar safe area so it does not
            // push the first icon row down from the native iPhone layout.
            Menu {
                if isEditingIcons {
                    Button(action: finishIconEditing) {
                        Label("Done", systemImage: "checkmark")
                    }
                } else {
                    Button(action: onExitHomeScreen) {
                        Label("Classic Stride Home", systemImage: "square.grid.2x2")
                    }
                }
                Button {
                    showHomeCustomization = true
                } label: {
                    Label("Customize", systemImage: "slider.horizontal.3")
                }
                Button {
                    showWallpaperGallery = true
                } label: {
                    Label("Choose Wallpaper", systemImage: "photo")
                }
                Button(action: onSettings) {
                    Label("Stride Settings", systemImage: "gearshape")
                }
                Button(action: onImport) {
                    Label("Add App", systemImage: "plus")
                }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.72))
                    .frame(width: 34, height: 34)
                    .contentShape(Circle())
            }
            .accessibilityLabel("Home Screen options")
            .padding(.trailing, 14)
            .padding(.top, 2)
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $showWallpaperGallery) { wallpaperGallery }
        .sheet(isPresented: $showHomeCustomization) { homeCustomizationSheet }
        .sheet(isPresented: $showPhotoPicker) {
            StrideHomePhotoPicker { image in saveCustomWallpaper(image) }
        }
        .animation(.easeInOut(duration: 0.2), value: isSearchPresented)
        .onChange(of: searchText) { _ in selectedPage = 0 }
        .onChange(of: packages.map { $0.packageDirectoryURL.path }) { _ in
            let required = max(1, (orderedPackages.count + 23) / 24)
            if storedPageCount < required { storedPageCount = required }
        }
        .onChange(of: isSearchPresented) { presented in
            if presented {
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
                    searchFocused = true
                }
            } else {
                searchFocused = false
                searchText = ""
            }
        }
    }

    private var topControls: some View {
        HStack {
            Spacer()
            Menu {
                Button(action: onImport) {
                    Label("Add App", systemImage: "plus")
                }
                Button(action: onSettings) {
                    Label("Stride Settings", systemImage: "gearshape")
                }
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 36, height: 36)
                    .background(.ultraThinMaterial, in: Circle())
                    .overlay(Circle().stroke(Color.white.opacity(0.13), lineWidth: 0.8))
                    .contentShape(Circle())
            }
            .accessibilityLabel("Home Screen options")
        }
    }

    private var activeSearchBar: some View {
        HStack(spacing: 9) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(.white.opacity(0.72))
            TextField("Search", text: $searchText)
                .font(.system(size: 15))
                .tint(.white)
                .foregroundStyle(.white)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .focused($searchFocused)
                .submitLabel(.search)
            Button("Cancel") {
                withAnimation(.easeInOut(duration: 0.18)) {
                    isSearchPresented = false
                }
            }
            .font(.system(size: 13, weight: .medium))
            .foregroundStyle(.white)
        }
        .padding(.horizontal, 14)
        .frame(height: 42)
        .background(.ultraThinMaterial, in: Capsule())
        .overlay(Capsule().stroke(Color.white.opacity(0.16), lineWidth: 0.8))
    }

    private var searchButton: some View {
        Button {
            withAnimation(.easeInOut(duration: 0.18)) {
                isSearchPresented = true
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 12, weight: .medium))
                Text("Search")
                    .font(.system(size: 12, weight: .medium))
            }
            .foregroundStyle(.white.opacity(0.88))
            .padding(.horizontal, 15)
            .frame(height: 29)
            .background(.ultraThinMaterial, in: Capsule())
            .overlay(Capsule().stroke(Color.white.opacity(0.13), lineWidth: 0.7))
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Search apps")
    }

    private var launcherPages: some View {
        TabView(selection: $selectedPage) {
            ForEach(Array(pages.enumerated()), id: \.offset) { entry in
                GeometryReader { geometry in
                    let horizontalInset: CGFloat = 18
                    let spacing: CGFloat = 18
                    let iconSize = min(largeIcons ? 66 : 60, max(largeIcons ? 60 : 56, (geometry.size.width - horizontalInset * 2 - spacing * 3) / 4))
                    LazyVGrid(columns: columns, alignment: .center, spacing: 20) {
                        ForEach(entry.element) { item in
                            appIcon(item, size: iconSize, globalIndex: entry.offset * 24 + entry.element.firstIndex(of: item)!)
                        }
                    }
                    .padding(.horizontal, horizontalInset)
                    .padding(.top, 15)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                    .contentShape(Rectangle())
                    .onDrop(of: ["public.text"], delegate: StrideHomePageDropDelegate(
                        destinationIndex: entry.offset * 24 + entry.element.count,
                        onDrop: movePackage
                    ))
                }
                .tag(entry.offset)
            }

            StrideAppLibraryScreen(
                packages: orderedPackages,
                accent: accent,
                onSelect: onSelect,
                onQuickLaunch: onQuickLaunch,
                onDelete: onDelete,
                onImport: onImport
            )
            .tag(pages.count)
        }
        .tabViewStyle(.page(indexDisplayMode: .never))
        .animation(.easeInOut(duration: 0.2), value: selectedPage)
        .onChange(of: pages.count) { count in
            if selectedPage > count { selectedPage = count }
        }
    }

    private func appIcon(_ item: ImportedAPK, size: CGFloat, globalIndex: Int) -> some View {
        Button { onSelect(item) } label: {
            VStack(spacing: largeIcons ? 0 : 5) {
                StrideLauncherIcon(data: item.iconData, accent: accent, size: size, appearance: iconAppearance)
                    .shadow(color: .black.opacity(0.28), radius: 5, y: 2)
                if !largeIcons {
                    Text(item.displayName)
                        .font(.system(size: 10, weight: .medium))
                        .foregroundStyle(.white)
                        .lineLimit(1)
                        .truncationMode(.tail)
                        .frame(maxWidth: .infinity)
                        .shadow(color: .black.opacity(0.78), radius: 3, y: 1)
                }
            }
            .contentShape(Rectangle())
        }
        .rotationEffect(.degrees(isJiggling ? (globalIndex.isMultiple(of: 2) ? 1.7 : -1.7) : 0))
        .animation(.easeInOut(duration: 0.13).repeatForever(autoreverses: true), value: isJiggling)
        .gesture(LongPressGesture(minimumDuration: 0.38).onChanged { _ in
            guard !isJiggling else { return }
            isJiggling = true
        }.onEnded { _ in
            isEditingIcons = true
        })
        .onDrag {
            draggedPackagePath = item.packageDirectoryURL.path
            isEditingIcons = true
            isJiggling = true
            return NSItemProvider(object: item.packageDirectoryURL.path as NSString)
        }
        .onDrop(of: ["public.text"], delegate: StrideHomeItemDropDelegate(
            destinationIndex: globalIndex,
            onDrop: movePackage
        ))
        .buttonStyle(.plain)
        .contextMenu {
            Button { onQuickLaunch(item) } label: {
                Label("Launch", systemImage: "play.fill")
            }
            Button { onSelect(item) } label: {
                Label("App Details", systemImage: "info.circle")
            }
            Button(role: .destructive) { onDelete(item) } label: {
                Label("Remove from Stride", systemImage: "trash")
            }
        }
        .accessibilityLabel(item.displayName)
        .accessibilityHint("Opens app details. Touch and hold for app actions.")
    }

    private var pageControls: some View {
        HStack(spacing: 12) {
            HStack(spacing: 7) {
                ForEach(0...pages.count, id: \.self) { index in
                    Capsule()
                        .fill(index == selectedPage ? Color.white : Color.white.opacity(0.38))
                        .frame(width: index == selectedPage ? 17 : 6, height: 6)
                        .contentShape(Rectangle().inset(by: -8))
                        .onTapGesture { withAnimation(.easeInOut(duration: 0.2)) { selectedPage = index } }
                        .onDrop(of: ["public.text"], delegate: StrideHomeItemDropDelegate(
                            destinationIndex: min(index, pages.count - 1) * 24,
                            onDrop: movePackage
                        ))
                }
            }
            .accessibilityLabel(selectedPage == pages.count
                ? "App Library"
                : "Home Screen page \(selectedPage + 1) of \(pages.count)")

            if selectedPage < pages.count {
                Button(action: addHomeScreenPage) {
                    Image(systemName: "plus")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(.white)
                        .frame(width: 22, height: 22)
                        .background(.ultraThinMaterial, in: Circle())
                        .overlay(Circle().stroke(Color.white.opacity(0.24), lineWidth: 0.8))
                }
                .accessibilityLabel("Add Home Screen page")
            }

            if isEditingIcons {
                Button("Done", action: finishIconEditing)
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(.white)
            }
        }
    }

    private var dock: some View {
        HStack(spacing: 0) {
            ForEach(dockPackages) { item in
                Button { onSelect(item) } label: {
                    StrideLauncherIcon(data: item.iconData, accent: accent, size: 60, appearance: iconAppearance)
                        .frame(maxWidth: .infinity)
                        .shadow(color: .black.opacity(0.26), radius: 5, y: 2)
                }
                .buttonStyle(.plain)
                .contextMenu {
                    Button { onQuickLaunch(item) } label: {
                        Label("Launch", systemImage: "play.fill")
                    }
                }
                .accessibilityLabel("Dock: \(item.displayName)")
            }
            if dockPackages.isEmpty {
                Button(action: onImport) {
                    Image(systemName: "plus.app.fill")
                        .font(.system(size: 34))
                        .foregroundStyle(.white.opacity(0.85))
                        .frame(maxWidth: .infinity)
                }
                .accessibilityLabel("Import your first app")
            }
        }
        .frame(height: 60)
        .padding(.horizontal, 18)
        .padding(.vertical, 15)
        // Native SwiftUI material samples the wallpaper behind the dock and
        // automatically adjusts blur, luminance and tint instead of using a
        // fixed glass color. This keeps the dock visually adaptive per wallpaper.
        .background {
            RoundedRectangle(cornerRadius: 30, style: .continuous)
                .fill(.ultraThinMaterial)
                .overlay {
                    RoundedRectangle(cornerRadius: 30, style: .continuous)
                        .fill(Color.primary.opacity(wallpaper == "black" ? 0.055 : 0.025))
                        .blendMode(.softLight)
                }
                .overlay {
                    RoundedRectangle(cornerRadius: 30, style: .continuous)
                        .strokeBorder(.white.opacity(wallpaper == "black" ? 0.14 : 0.23), lineWidth: 0.8)
                }
                .shadow(color: .black.opacity(0.22), radius: 18, y: 8)
        }
        .padding(.horizontal, 17)
        .padding(.bottom, 7)
    }

    private var emptyState: some View {
        VStack(spacing: 13) {
            Image(systemName: searchText.isEmpty ? "square.grid.2x2" : "magnifyingglass")
                .font(.system(size: 38, weight: .light))
                .foregroundStyle(.white.opacity(0.9))
            Text(searchText.isEmpty ? "Your Home Screen is empty" : "No apps found")
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(.white)
            Text(searchText.isEmpty
                 ? "Add an Android app to place its icon here."
                 : "Try a different app name.")
                .font(.system(size: 12))
                .foregroundStyle(.white.opacity(0.72))
                .multilineTextAlignment(.center)
            Button(action: searchText.isEmpty ? onImport : { searchText = "" }) {
                Text(searchText.isEmpty ? "Add App" : "Clear Search")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(.black)
                    .padding(.horizontal, 20)
                    .padding(.vertical, 10)
                    .background(Color.white, in: Capsule())
            }
            .padding(.top, 3)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(.horizontal, 30)
    }
}


private struct StrideHomeItemDropDelegate: DropDelegate {
    let destinationIndex: Int
    let onDrop: (String, Int) -> Void

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        guard let provider = info.itemProviders(for: ["public.text"]).first else { return false }
        provider.loadObject(ofClass: NSString.self) { object, _ in
            guard let path = object as? String else { return }
            DispatchQueue.main.async { onDrop(path, destinationIndex) }
        }
        return true
    }
}

private struct StrideHomePageDropDelegate: DropDelegate {
    let destinationIndex: Int
    let onDrop: (String, Int) -> Void

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func performDrop(info: DropInfo) -> Bool {
        guard let provider = info.itemProviders(for: ["public.text"]).first else { return false }
        provider.loadObject(ofClass: NSString.self) { object, _ in
            guard let path = object as? String else { return }
            DispatchQueue.main.async { onDrop(path, destinationIndex) }
        }
        return true
    }
}

private struct StrideLauncherIcon: View {
    let data: Data?
    let accent: Color
    let size: CGFloat
    var appearance: String = "automatic"

    /// Removes transparent padding from APK launcher images before placing them
    /// into the fixed iOS icon cell. This keeps artwork optically consistent even
    /// when Android resources include oversized transparent canvases.
    private var normalizedImage: UIImage? {
        guard let data, let source = UIImage(data: data), let cgImage = source.cgImage else { return nil }
        let width = cgImage.width
        let height = cgImage.height
        let (pixelCount, overflow) = width.multipliedReportingOverflow(by: height)
        guard width > 0, height > 0, !overflow, pixelCount <= 16_777_216 else { return source }

        var pixels = [UInt8](repeating: 0, count: pixelCount * 4)
        let rendered = pixels.withUnsafeMutableBytes { raw -> Bool in
            guard let context = CGContext(
                data: raw.baseAddress,
                width: width,
                height: height,
                bitsPerComponent: 8,
                bytesPerRow: width * 4,
                space: CGColorSpaceCreateDeviceRGB(),
                bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue | CGBitmapInfo.byteOrder32Big.rawValue
            ) else { return false }
            context.draw(cgImage, in: CGRect(x: 0, y: 0, width: width, height: height))
            return true
        }
        guard rendered else { return source }

        var minX = width
        var minY = height
        var maxX = -1
        var maxY = -1
        for y in 0..<height {
            for x in 0..<width where pixels[(y * width + x) * 4 + 3] > 12 {
                minX = min(minX, x)
                minY = min(minY, y)
                maxX = max(maxX, x)
                maxY = max(maxY, y)
            }
        }
        guard maxX >= minX, maxY >= minY else { return source }
        let visibleWidth = maxX - minX + 1
        let visibleHeight = maxY - minY + 1
        guard visibleWidth < width || visibleHeight < height else { return source }

        let padX = max(1, Int(Double(visibleWidth) * 0.015))
        let padY = max(1, Int(Double(visibleHeight) * 0.015))
        let crop = CGRect(
            x: max(0, minX - padX),
            y: max(0, minY - padY),
            width: min(width, maxX + padX + 1) - max(0, minX - padX),
            height: min(height, maxY + padY + 1) - max(0, minY - padY)
        )
        guard let trimmed = cgImage.cropping(to: crop) else { return source }
        return UIImage(cgImage: trimmed, scale: source.scale, orientation: .up)
    }

    var body: some View {
        Group {
            if let image = normalizedImage {
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.high)
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipped()
                    .overlay {
                        if appearance == "dark" {
                            RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
                                .fill(Color.black.opacity(0.22))
                                .blendMode(.multiply)
                        } else if appearance == "tinted" {
                            RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
                                .fill(accent.opacity(0.30))
                                .blendMode(.color)
                        } else if appearance == "clear" {
                            RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
                                .fill(Color.white.opacity(0.14))
                        }
                    }
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.225, style: .continuous)
                        .fill(LinearGradient(colors: [accent.opacity(0.85), accent.opacity(0.42)],
                                             startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "app.fill")
                        .font(.system(size: size * 0.42, weight: .medium))
                        .foregroundStyle(.white)
                }
                .frame(width: size, height: size)
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size * 0.225, style: .continuous))
    }
}

private func stridePages<T>(_ values: [T], size: Int) -> [[T]] {
    guard size > 0 else { return [] }
    return stride(from: 0, to: values.count, by: size).map {
        Array(values[$0..<min($0 + size, values.count)])
    }
}


/// Uses the system photo picker, so the user can select a wallpaper without
/// granting Stride broad access to the photo library (iOS 14+).
private struct StrideHomePhotoPicker: UIViewControllerRepresentable {
    let onImageSelected: (UIImage) -> Void

    func makeUIViewController(context: Context) -> PHPickerViewController {
        var configuration = PHPickerConfiguration()
        configuration.filter = .images
        configuration.selectionLimit = 1
        let picker = PHPickerViewController(configuration: configuration)
        picker.delegate = context.coordinator
        return picker
    }

    func updateUIViewController(_ controller: PHPickerViewController, context: Context) {}

    func makeCoordinator() -> Coordinator {
        Coordinator(onImageSelected: onImageSelected)
    }

    final class Coordinator: NSObject, PHPickerViewControllerDelegate {
        let onImageSelected: (UIImage) -> Void

        init(onImageSelected: @escaping (UIImage) -> Void) {
            self.onImageSelected = onImageSelected
        }

        func picker(_ picker: PHPickerViewController, didFinishPicking results: [PHPickerResult]) {
            picker.dismiss(animated: true)
            guard let provider = results.first?.itemProvider,
                  provider.canLoadObject(ofClass: UIImage.self) else { return }
            provider.loadObject(ofClass: UIImage.self) { [weak self] object, _ in
                guard let image = object as? UIImage else { return }
                DispatchQueue.main.async { self?.onImageSelected(image) }
            }
        }
    }
}
