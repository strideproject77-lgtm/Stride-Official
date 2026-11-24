import SwiftUI
import UIKit

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

    @AppStorage("stride.iosHome.wallpaper") private var wallpaper = "black"
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

    private var wallpaperColors: [Color] {
        switch wallpaper {
        case "ocean":
            return [Color(red: 0.025, green: 0.20, blue: 0.34), Color(red: 0.025, green: 0.045, blue: 0.19)]
        case "forest":
            return [Color(red: 0.035, green: 0.23, blue: 0.19), Color(red: 0.012, green: 0.045, blue: 0.075)]
        case "black":
            return [Color.black, Color.black]
        case "dusk":
            return [Color(red: 0.34, green: 0.17, blue: 0.27), Color(red: 0.055, green: 0.045, blue: 0.16)]
        default:
            return [Color(red: 0.10, green: 0.24, blue: 0.56), Color(red: 0.28, green: 0.12, blue: 0.48)]
        }
    }

    private var wallpaperBackground: some View {
        GeometryReader { geometry in
            ZStack {
                if wallpaper == "black" {
                    Color.black
                } else {
                    LinearGradient(colors: wallpaperColors, startPoint: .topLeading, endPoint: .bottomTrailing)
                    Circle()
                        .fill(RadialGradient(colors: [Color(red: 0.38, green: 0.78, blue: 1).opacity(0.78), .clear],
                                             center: .center, startRadius: 2, endRadius: geometry.size.width * 0.72))
                        .frame(width: geometry.size.width * 1.15, height: geometry.size.width * 1.15)
                        .blur(radius: 18)
                        .offset(x: geometry.size.width * 0.34, y: -geometry.size.height * 0.25)
                    Ellipse()
                        .fill(RadialGradient(colors: [Color(red: 0.78, green: 0.40, blue: 1).opacity(0.72), .clear],
                                             center: .center, startRadius: 2, endRadius: geometry.size.width * 0.62))
                        .frame(width: geometry.size.width * 1.1, height: geometry.size.height * 0.56)
                        .blur(radius: 24)
                        .offset(x: -geometry.size.width * 0.30, y: geometry.size.height * 0.23)
                    Ellipse()
                        .fill(RadialGradient(colors: [Color.white.opacity(0.28), .clear],
                                             center: .center, startRadius: 1, endRadius: geometry.size.width * 0.44))
                        .frame(width: geometry.size.width * 0.9, height: geometry.size.height * 0.28)
                        .blur(radius: 28)
                        .offset(x: geometry.size.width * 0.12, y: geometry.size.height * 0.02)
                }
            }
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
        }
        .ignoresSafeArea()
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
        Group {
            if filteredPackages.isEmpty {
                emptyState
            } else {
                TabView(selection: $selectedPage) {
                    ForEach(Array(pages.enumerated()), id: \.offset) { entry in
                        GeometryReader { geometry in
                            let horizontalInset: CGFloat = 18
                            let spacing: CGFloat = 18
                            let iconSize = min(64, max(52, (geometry.size.width - horizontalInset * 2 - spacing * 3) / 4))
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
                }
                .tabViewStyle(.page(indexDisplayMode: .never))
                .animation(.easeInOut(duration: 0.2), value: selectedPage)
            }
        }
    }

    private func appIcon(_ item: ImportedAPK, size: CGFloat, globalIndex: Int) -> some View {
        Button { onSelect(item) } label: {
            VStack(spacing: 5) {
                StrideLauncherIcon(data: item.iconData, accent: accent, size: size)
                    .shadow(color: .black.opacity(0.28), radius: 5, y: 2)
                Text(item.displayName)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity)
                    .shadow(color: .black.opacity(0.78), radius: 3, y: 1)
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
                ForEach(pages.indices, id: \.self) { index in
                    Capsule()
                        .fill(index == selectedPage ? Color.white : Color.white.opacity(0.38))
                        .frame(width: index == selectedPage ? 17 : 6, height: 6)
                        .contentShape(Rectangle().inset(by: -8))
                        .onTapGesture { withAnimation(.easeInOut(duration: 0.2)) { selectedPage = index } }
                        .onDrop(of: ["public.text"], delegate: StrideHomeItemDropDelegate(
                            destinationIndex: index * 24,
                            onDrop: movePackage
                        ))
                }
            }
            .accessibilityLabel("Home Screen page \(selectedPage + 1) of \(pages.count)")

            Button(action: addHomeScreenPage) {
                Image(systemName: "plus")
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(.white)
                    .frame(width: 22, height: 22)
                    .background(.ultraThinMaterial, in: Circle())
                    .overlay(Circle().stroke(Color.white.opacity(0.24), lineWidth: 0.8))
            }
            .accessibilityLabel("Add Home Screen page")

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
                    StrideLauncherIcon(data: item.iconData, accent: accent, size: 64)
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
        .frame(height: 64)
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
