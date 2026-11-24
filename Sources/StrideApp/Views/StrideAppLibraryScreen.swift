import SwiftUI
import UIKit

/// App Library presentation following the iOS 27 category-first layout and Liquid Glass visual language.
struct StrideAppLibraryScreen: View {
    let packages: [ImportedAPK]
    let accent: Color
    let onSelect: (ImportedAPK) -> Void
    let onQuickLaunch: (ImportedAPK) -> Void
    let onDelete: (ImportedAPK) -> Void
    let onImport: () -> Void

    @State private var searchText = ""
    @FocusState private var searchFocused: Bool

    private let twoColumns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]
    private let iconColumns = [GridItem(.flexible(), spacing: 6), GridItem(.flexible(), spacing: 6)]

    private struct LibraryCategory: Identifiable {
        let title: String
        let symbol: String
        let apps: [ImportedAPK]
        var id: String { title }
    }

    private var recentlyAdded: [ImportedAPK] {
        Array(packages.sorted {
            let first = (try? $0.packageDirectoryURL.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? .distantPast
            let second = (try? $1.packageDirectoryURL.resourceValues(forKeys: [.creationDateKey]).creationDate) ?? .distantPast
            return first > second
        }.prefix(4))
    }

    private var suggested: [ImportedAPK] {
        let ranked = packages.sorted {
            let first = UserDefaults.standard.object(forKey: lastPlayedKey(for: $0)) as? Date ?? .distantPast
            let second = UserDefaults.standard.object(forKey: lastPlayedKey(for: $1)) as? Date ?? .distantPast
            return first > second
        }
        let used = ranked.filter { UserDefaults.standard.object(forKey: lastPlayedKey(for: $0)) != nil }
        return Array((used.isEmpty ? packages : used).prefix(4))
    }

    private var categories: [LibraryCategory] {
        let definitions: [(String, String, [String])] = [
            ("Utilities", "wrench.and.screwdriver.fill", ["tool", "file", "calculator", "clock", "settings", "browser", "vpn", "store"]),
            ("Creativity", "paintpalette.fill", ["camera", "photo", "draw", "editor", "design", "paint"]),
            ("Social", "person.2.fill", ["chat", "social", "message", "telegram", "discord", "whatsapp", "instagram", "facebook"]),
            ("Entertainment", "play.rectangle.fill", ["video", "music", "player", "stream", "movie", "tv", "media"]),
            ("Productivity & Finance", "briefcase.fill", ["office", "document", "sheet", "finance", "bank", "wallet", "pay"]),
            ("Information & Reading", "book.fill", ["book", "news", "reader", "wiki"]),
            ("Games", "gamecontroller.fill", ["game", "racing", "surfer", "hill", "driving", "2048", "pool", "clash", "subway", "angry", "car", "football", "soccer"])
        ]
        var assigned = Set<String>()
        var result: [LibraryCategory] = definitions.map { name, symbol, terms in
            let matches = packages.filter { item in
                let key = item.packageDirectoryURL.standardizedFileURL.path
                guard !assigned.contains(key) else { return false }
                let title = item.displayName.lowercased()
                guard terms.contains(where: { title.contains($0) }) else { return false }
                assigned.insert(key)
                return true
            }
            return LibraryCategory(title: name, symbol: symbol, apps: matches)
        }.filter { !$0.apps.isEmpty }

        let others = packages.filter { !assigned.contains($0.packageDirectoryURL.standardizedFileURL.path) }
        if !others.isEmpty {
            result.append(LibraryCategory(title: "Other", symbol: "square.grid.2x2.fill", apps: others))
        }
        return result
    }

    private var matchingApps: [ImportedAPK] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return packages.sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending } }
        return packages.filter { $0.displayName.localizedCaseInsensitiveContains(query) }
            .sorted { $0.displayName.localizedCaseInsensitiveCompare($1.displayName) == .orderedAscending }
    }

    private func lastPlayedKey(for item: ImportedAPK) -> String {
        "stride.lastPlayed.\(item.packageDirectoryURL.standardizedFileURL.path)"
    }

    var body: some View {
        ZStack {
            libraryBackdrop
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 18) {
                    searchBar

                    if searchFocused || !searchText.isEmpty {
                        alphabeticalResults
                    } else if packages.isEmpty {
                        emptyLibrary
                    } else {
                        HStack(alignment: .top, spacing: 12) {
                            previewPanel("Suggestions", apps: suggested)
                            previewPanel("Recently Added", apps: recentlyAdded)
                        }

                        VStack(alignment: .leading, spacing: 11) {
                            Text("CATEGORIES")
                                .font(.system(size: 10, weight: .bold))
                                .tracking(1.25)
                                .foregroundStyle(.white.opacity(0.62))
                                .padding(.horizontal, 2)

                            LazyVGrid(columns: twoColumns, spacing: 12) {
                                ForEach(categories) { category in
                                    categoryPanel(category)
                                }
                            }
                        }
                    }
                }
                .padding(.horizontal, 17)
                .padding(.top, 10)
                .padding(.bottom, 30)
            }
        }
        .preferredColorScheme(.dark)
        .sheet(item: $selectedCategory) { selected in
            categoryDetail(selected)
        }
    }

    private var libraryBackdrop: some View {
        ZStack {
            LinearGradient(
                colors: [
                    Color(red: 0.035, green: 0.055, blue: 0.105),
                    Color(red: 0.018, green: 0.025, blue: 0.055),
                    Color(red: 0.045, green: 0.035, blue: 0.075)
                ],
                startPoint: .topLeading,
                endPoint: .bottomTrailing
            )
            Circle()
                .fill(Color(red: 0.20, green: 0.42, blue: 0.72).opacity(0.24))
                .frame(width: 310, height: 310)
                .blur(radius: 90)
                .offset(x: 150, y: -310)
            Circle()
                .fill(Color(red: 0.48, green: 0.25, blue: 0.62).opacity(0.17))
                .frame(width: 280, height: 280)
                .blur(radius: 85)
                .offset(x: -150, y: 280)
        }
        .ignoresSafeArea()
    }

    private var searchBar: some View {
        HStack(spacing: 9) {
            Image(systemName: "magnifyingglass")
                .font(.system(size: 16, weight: .medium))
                .foregroundStyle(.white.opacity(0.82))
            TextField("App Library", text: $searchText)
                .font(.system(size: 16, weight: .regular))
                .foregroundStyle(.white)
                .tint(.white)
                .autocorrectionDisabled()
                .textInputAutocapitalization(.never)
                .focused($searchFocused)
            if !searchText.isEmpty {
                Button {
                    searchText = ""
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.white.opacity(0.65))
                }
                .buttonStyle(.plain)
            }
            Button(action: onImport) {
                Image(systemName: "plus")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(.white.opacity(0.9))
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Import app")
        }
        .padding(.horizontal, 16)
        .frame(height: 49)
        .strideLiquidGlass(in: Capsule())
        .overlay(Capsule().stroke(.white.opacity(0.28), lineWidth: 0.8))
        .shadow(color: .black.opacity(0.18), radius: 12, y: 5)
    }

    private func previewPanel(_ title: String, apps: [ImportedAPK]) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            Text(title)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(.white.opacity(0.92))
                .lineLimit(1)
                .minimumScaleFactor(0.8)
            LazyVGrid(columns: iconColumns, spacing: 7) {
                ForEach(Array(apps.prefix(4))) { item in
                    appIconButton(item, size: 47)
                }
                ForEach(0..<max(0, 4 - min(4, apps.count)), id: \.self) { _ in
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(.white.opacity(0.055))
                        .frame(height: 47)
                }
            }
        }
        .padding(11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .strideLiquidGlass(in: RoundedRectangle(cornerRadius: 23, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 23, style: .continuous).stroke(.white.opacity(0.20), lineWidth: 0.8))
        .shadow(color: .black.opacity(0.16), radius: 10, y: 5)
    }

    private func categoryPanel(_ category: LibraryCategory) -> some View {
        VStack(alignment: .leading, spacing: 9) {
            LazyVGrid(columns: iconColumns, spacing: 7) {
                ForEach(Array(category.apps.prefix(4))) { item in
                    appIconButton(item, size: 48)
                }
                ForEach(0..<max(0, 4 - min(4, category.apps.count)), id: \.self) { _ in
                    RoundedRectangle(cornerRadius: 12, style: .continuous)
                        .fill(.white.opacity(0.055))
                        .frame(height: 48)
                }
            }
            HStack(spacing: 4) {
                Text(category.title)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(.white)
                    .lineLimit(1)
                    .minimumScaleFactor(0.78)
                Spacer(minLength: 0)
                Text("\(category.apps.count)")
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(.white.opacity(0.65))
            }
        }
        .padding(11)
        .frame(maxWidth: .infinity, alignment: .leading)
        .strideLiquidGlass(in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .overlay(RoundedRectangle(cornerRadius: 22, style: .continuous).stroke(.white.opacity(0.18), lineWidth: 0.8))
        .contentShape(RoundedRectangle(cornerRadius: 22, style: .continuous))
        .onTapGesture {
            // Category cards expand through a dedicated full-screen sheet below.
            selectedCategory = category
        }
    }

    @State private var selectedCategory: LibraryCategory?

    private func categoryDetail(_ category: LibraryCategory) -> some View {
        NavigationView {
            ScrollView {
                LazyVGrid(columns: twoColumns, spacing: 18) {
                    ForEach(category.apps) { item in
                        Button { onSelect(item) } label: {
                            VStack(spacing: 7) {
                                StrideAppLibraryIcon(data: item.iconData, accent: accent, size: 58)
                                Text(item.displayName)
                                    .font(.system(size: 12, weight: .medium))
                                    .foregroundStyle(.primary)
                                    .lineLimit(2)
                                    .multilineTextAlignment(.center)
                            }
                            .frame(maxWidth: .infinity)
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button { onQuickLaunch(item) } label: { Label("Launch", systemImage: "play.fill") }
                            Button(role: .destructive) { onDelete(item) } label: { Label("Remove", systemImage: "trash") }
                        }
                    }
                }
                .padding(18)
            }
            .navigationTitle(category.title)
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button("Done") { selectedCategory = nil }
                }
            }
        }
    }

    private var alphabeticalResults: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Text(searchText.isEmpty ? "ALL APPS" : "SEARCH RESULTS")
                    .font(.system(size: 10, weight: .bold))
                    .tracking(1.2)
                    .foregroundStyle(.white.opacity(0.62))
                Spacer()
                Text("\(matchingApps.count)")
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.white.opacity(0.55))
            }
            if matchingApps.isEmpty {
                Text("No apps found")
                    .font(.system(size: 14, weight: .medium))
                    .foregroundStyle(.white.opacity(0.7))
                    .padding(.vertical, 24)
            } else {
                LazyVStack(spacing: 0) {
                    ForEach(matchingApps) { item in
                        Button { onSelect(item) } label: {
                            HStack(spacing: 12) {
                                StrideAppLibraryIcon(data: item.iconData, accent: accent, size: 39)
                                Text(item.displayName)
                                    .font(.system(size: 14, weight: .medium))
                                    .foregroundStyle(.white)
                                Spacer()
                                Image(systemName: "chevron.right")
                                    .font(.system(size: 11, weight: .semibold))
                                    .foregroundStyle(.white.opacity(0.35))
                            }
                            .padding(.vertical, 9)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .contextMenu {
                            Button { onQuickLaunch(item) } label: { Label("Launch", systemImage: "play.fill") }
                            Button(role: .destructive) { onDelete(item) } label: { Label("Remove", systemImage: "trash") }
                        }
                        if item.id != matchingApps.last?.id {
                            Divider().overlay(.white.opacity(0.10))
                        }
                    }
                }
                .padding(.horizontal, 12)
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
                .overlay(RoundedRectangle(cornerRadius: 20).stroke(.white.opacity(0.15), lineWidth: 0.8))
            }
        }
    }

    private var emptyLibrary: some View {
        VStack(spacing: 12) {
            Image(systemName: "square.grid.2x2")
                .font(.system(size: 34, weight: .light))
                .foregroundStyle(.white.opacity(0.7))
            Text("Your App Library is empty")
                .font(.system(size: 17, weight: .semibold))
                .foregroundStyle(.white)
            Text("Apps you import will appear here automatically.")
                .font(.system(size: 13))
                .foregroundStyle(.white.opacity(0.6))
                .multilineTextAlignment(.center)
            Button(action: onImport) {
                Label("Import App", systemImage: "plus")
                    .font(.system(size: 14, weight: .semibold))
                    .padding(.horizontal, 18)
                    .padding(.vertical, 11)
                    .background(accent, in: Capsule())
                    .foregroundStyle(.white)
            }
        }
        .frame(maxWidth: .infinity)
        .padding(.vertical, 50)
    }

    private func appIconButton(_ item: ImportedAPK, size: CGFloat) -> some View {
        Button { onSelect(item) } label: {
            StrideAppLibraryIcon(data: item.iconData, accent: accent, size: size)
                .frame(maxWidth: .infinity)
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button { onQuickLaunch(item) } label: { Label("Launch", systemImage: "play.fill") }
            Button(role: .destructive) { onDelete(item) } label: { Label("Remove", systemImage: "trash") }
        }
    }
}

private struct StrideAppLibraryIcon: View {
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
                    .padding(size * 0.035)
                    .background(Color.white.opacity(0.025))
            } else {
                ZStack {
                    RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                        .fill(LinearGradient(colors: [accent.opacity(0.85), accent.opacity(0.4)], startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "app.fill")
                        .font(.system(size: size * 0.42))
                        .foregroundStyle(.white)
                }
            }
        }
        .frame(width: size, height: size)
        .clipShape(RoundedRectangle(cornerRadius: size * 0.22, style: .continuous))
    }
}


private extension View {
    @ViewBuilder
    func strideLiquidGlass<S: Shape>(in shape: S) -> some View {
        if #available(iOS 26.0, *) {
            self.glassEffect(.regular, in: shape)
        } else {
            self.background(.ultraThinMaterial, in: shape)
        }
    }
}
