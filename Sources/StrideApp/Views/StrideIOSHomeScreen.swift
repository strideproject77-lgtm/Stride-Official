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

    @AppStorage("stride.iosHome.wallpaper") private var wallpaper = "dusk"
    @State private var searchText = ""
    @State private var isSearchPresented = false
    @State private var selectedPage = 0
    @FocusState private var searchFocused: Bool

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 9), count: 4)

    private var filteredPackages: [ImportedAPK] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return packages }
        return packages.filter { $0.displayName.localizedCaseInsensitiveContains(query) }
    }

    // A 4 × 6 page mirrors the standard iPhone Home Screen grid.
    private var pages: [[ImportedAPK]] {
        stridePages(filteredPackages, size: 24)
    }

    private var dockPackages: [ImportedAPK] {
        Array(packages.prefix(4))
    }

    private var wallpaperColors: [Color] {
        switch wallpaper {
        case "ocean":
            return [Color(red: 0.025, green: 0.20, blue: 0.34), Color(red: 0.025, green: 0.045, blue: 0.19)]
        case "forest":
            return [Color(red: 0.035, green: 0.23, blue: 0.19), Color(red: 0.012, green: 0.045, blue: 0.075)]
        case "black":
            return [Color(red: 0.035, green: 0.045, blue: 0.085), Color.black]
        case "dusk":
            return [Color(red: 0.34, green: 0.17, blue: 0.27), Color(red: 0.055, green: 0.045, blue: 0.16)]
        default:
            return [Color(red: 0.10, green: 0.24, blue: 0.56), Color(red: 0.28, green: 0.12, blue: 0.48)]
        }
    }

    private var wallpaperBackground: some View {
        GeometryReader { geometry in
            ZStack {
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
            .frame(width: geometry.size.width, height: geometry.size.height)
            .clipped()
        }
        .ignoresSafeArea()
    }

    var body: some View {
        ZStack {
            wallpaperBackground

            VStack(spacing: 0) {
                topControls
                    .padding(.horizontal, 22)
                    .padding(.top, 5)

                launcherPages
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
                } else if pages.count > 1 {
                    pageIndicator
                        .padding(.bottom, 9)
                }

                dock
                    .padding(.horizontal, 18)
                    .padding(.vertical, 10)
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
        .preferredColorScheme(.dark)
        .animation(.easeInOut(duration: 0.2), value: isSearchPresented)
        .onChange(of: searchText) { _ in selectedPage = 0 }
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
                            let horizontalInset: CGFloat = 17
                            let spacing: CGFloat = 9
                            let iconSize = min(62, max(56, (geometry.size.width - horizontalInset * 2 - spacing * 3) / 4))
                            LazyVGrid(columns: columns, alignment: .center, spacing: 19) {
                                ForEach(entry.element) { item in
                                    appIcon(item, size: iconSize)
                                }
                            }
                            .padding(.horizontal, horizontalInset)
                            .padding(.top, 15)
                            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .top)
                        }
                        .tag(entry.offset)
                    }
                }
                .tabViewStyle(.page(indexDisplayMode: .never))
                .animation(.easeInOut(duration: 0.2), value: selectedPage)
            }
        }
    }

    private func appIcon(_ item: ImportedAPK, size: CGFloat) -> some View {
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

    private var pageIndicator: some View {
        HStack(spacing: 7) {
            ForEach(pages.indices, id: \.self) { index in
                Capsule()
                    .fill(index == selectedPage ? Color.white : Color.white.opacity(0.38))
                    .frame(width: index == selectedPage ? 17 : 6, height: 6)
            }
        }
        .accessibilityLabel("Home Screen page \(selectedPage + 1) of \(pages.count)")
    }

    private var dock: some View {
        HStack(spacing: 0) {
            ForEach(dockPackages) { item in
                Button { onSelect(item) } label: {
                    StrideLauncherIcon(data: item.iconData, accent: accent, size: 58)
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
        .frame(height: 58)
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

private struct StrideLauncherIcon: View {
    let data: Data?
    let accent: Color
    let size: CGFloat

    var body: some View {
        Group {
            if let data, let image = UIImage(data: data) {
                Image(uiImage: image)
                    .resizable()
                    .interpolation(.high)
                    // Preserve the complete APK icon. Fill+clipped crops artwork and is
                    // intentionally avoided; fit keeps the original aspect ratio visible.
                    .scaledToFit()
                    .frame(width: size, height: size)
                    .background(Color.black.opacity(0.08))
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.23, style: .continuous)
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
        .clipShape(RoundedRectangle(cornerRadius: size * 0.235, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: size * 0.235, style: .continuous)
                .fill(LinearGradient(
                    colors: [Color.white.opacity(0.10), .clear, Color.black.opacity(0.055)],
                    startPoint: .topLeading, endPoint: .bottomTrailing
                ))
                .allowsHitTesting(false)
        }
        .overlay(RoundedRectangle(cornerRadius: size * 0.235, style: .continuous)
            .stroke(Color.white.opacity(0.14), lineWidth: 0.75))
    }
}

private func stridePages<T>(_ values: [T], size: Int) -> [[T]] {
    guard size > 0 else { return [] }
    return stride(from: 0, to: values.count, by: size).map {
        Array(values[$0..<min($0 + size, values.count)])
    }
}
