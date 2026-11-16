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
    @State private var selectedPage = 0

    private let columns = Array(repeating: GridItem(.flexible(), spacing: 8), count: 4)

    private var filteredPackages: [ImportedAPK] {
        guard !searchText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else { return packages }
        return packages.filter { $0.displayName.localizedCaseInsensitiveContains(searchText) }
    }

    private var pages: [[ImportedAPK]] {
        stridePages(filteredPackages, size: 16)
    }

    private var dockPackages: [ImportedAPK] {
        Array(packages.prefix(4))
    }

    private var wallpaperColors: [Color] {
        switch wallpaper {
        case "ocean":
            return [Color(red: 0.025, green: 0.12, blue: 0.24), Color(red: 0.02, green: 0.035, blue: 0.12)]
        case "forest":
            return [Color(red: 0.04, green: 0.19, blue: 0.15), Color(red: 0.015, green: 0.055, blue: 0.06)]
        case "black":
            return [Color.black, Color(red: 0.025, green: 0.025, blue: 0.035)]
        default:
            return [Color(red: 0.30, green: 0.14, blue: 0.19), Color(red: 0.055, green: 0.045, blue: 0.13)]
        }
    }

    var body: some View {
        ZStack {
            LinearGradient(colors: wallpaperColors, startPoint: .topLeading, endPoint: .bottomTrailing)
                .ignoresSafeArea()
            VStack(spacing: 0) {
                topBar
                searchBar
                    .padding(.horizontal, 22)
                    .padding(.top, 12)

                if filteredPackages.isEmpty {
                    emptyState
                } else {
                    launcherPages
                }

                Spacer(minLength: 8)
                if !searchText.isEmpty {
                    Text("\(filteredPackages.count) RESULTS")
                        .font(.system(size: 9, weight: .bold, design: .rounded))
                        .tracking(1.2)
                        .foregroundStyle(.white.opacity(0.62))
                        .padding(.bottom, 9)
                } else if pages.count > 1 {
                    pageIndicator
                        .padding(.bottom, 10)
                }

                dock
                    .padding(.horizontal, 18)
                    .padding(.vertical, 12)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 27, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 27, style: .continuous)
                        .stroke(Color.white.opacity(0.16), lineWidth: 1))
                    .padding(.horizontal, 18)
                    .padding(.bottom, 9)
            }
        }
        .preferredColorScheme(.dark)
        .onChange(of: searchText) { _, _ in selectedPage = 0 }
    }

    private var topBar: some View {
        HStack(alignment: .center) {
            VStack(alignment: .leading, spacing: 3) {
                Text("Stride")
                    .font(.system(size: 30, weight: .bold, design: .rounded))
                    .tracking(-1.2)
                    .foregroundStyle(.white)
                Text(Date.now.formatted(.dateTime.weekday(.wide).month(.abbreviated).day()))
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.white.opacity(0.72))
            }
            Spacer()
            Button(action: onImport) {
                Image(systemName: "plus")
                    .font(.system(size: 16, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 40, height: 40)
                    .background(.ultraThinMaterial, in: Circle())
            }
            .accessibilityLabel("Import Android app")
            Button(action: onSettings) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(.white)
                    .frame(width: 40, height: 40)
                    .background(.ultraThinMaterial, in: Circle())
            }
            .accessibilityLabel("Stride settings")
        }
        .padding(.horizontal, 23)
        .padding(.top, 8)
    }

    private var searchBar: some View {
        HStack(spacing: 9) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(.white.opacity(0.72))
            TextField("Search apps", text: $searchText)
                .font(.system(size: 14))
                .tint(.white)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
            if !searchText.isEmpty {
                Button { searchText = "" } label: {
                    Image(systemName: "xmark.circle.fill").foregroundStyle(.white.opacity(0.7))
                }
                .accessibilityLabel("Clear search")
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 43)
        .background(.ultraThinMaterial, in: Capsule())
        .overlay(Capsule().stroke(Color.white.opacity(0.14), lineWidth: 1))
    }

    private var launcherPages: some View {
        TabView(selection: $selectedPage) {
            ForEach(Array(pages.enumerated()), id: \.offset) { pageIndex, page in
                LazyVGrid(columns: columns, alignment: .center, spacing: 19) {
                    ForEach(page) { item in
                        appIcon(item)
                    }
                }
                .padding(.horizontal, 17)
                .padding(.top, 24)
                .frame(maxHeight: .infinity, alignment: .top)
                .tag(pageIndex)
            }
        }
        .tabViewStyle(.page(indexDisplayMode: .never))
        .animation(.easeInOut(duration: 0.2), value: selectedPage)
    }

    private func appIcon(_ item: ImportedAPK) -> some View {
        Button { onSelect(item) } label: {
            VStack(spacing: 6) {
                StrideLauncherIcon(data: item.iconData, accent: accent, size: 61)
                    .shadow(color: .black.opacity(0.24), radius: 5, y: 3)
                Text(item.displayName)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity)
                    .shadow(color: .black.opacity(0.75), radius: 3, y: 1)
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
        .accessibilityLabel("Home screen page \(selectedPage + 1) of \(pages.count)")
    }

    private var dock: some View {
        HStack(spacing: 0) {
            ForEach(dockPackages) { item in
                Button { onSelect(item) } label: {
                    StrideLauncherIcon(data: item.iconData, accent: accent, size: 53)
                        .frame(maxWidth: .infinity)
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
                 ? "Import an Android APK to add its app icon here."
                 : "Try a different app name.")
                .font(.system(size: 12))
                .foregroundStyle(.white.opacity(0.72))
                .multilineTextAlignment(.center)
            Button(action: searchText.isEmpty ? onImport : { searchText = "" }) {
                Text(searchText.isEmpty ? "Import App" : "Clear Search")
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
                    .scaledToFill()
                    .frame(width: size, height: size)
                    .clipped()
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.23, style: .continuous)
                        .fill(LinearGradient(colors: [accent.opacity(0.85), accent.opacity(0.42)],
                                             startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "app.fill")
                        .font(.system(size: size * 0.42, weight: .medium))
                        .foregroundStyle(.white)
                }
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size * 0.23, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: size * 0.23, style: .continuous)
            .stroke(Color.white.opacity(0.12), lineWidth: 0.7))
    }
}

private func stridePages<T>(_ values: [T], size: Int) -> [[T]] {
    guard size > 0 else { return [] }
    return stride(from: 0, to: values.count, by: size).map {
        Array(values[$0..<min($0 + size, values.count)])
    }
}
