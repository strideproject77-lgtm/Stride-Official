import SwiftUI

/// iOS-style App Library for packages already stored by Stride.
struct StrideAppLibraryScreen: View {
    let packages: [ImportedAPK]
    let accent: Color
    let onSelect: (ImportedAPK) -> Void
    let onQuickLaunch: (ImportedAPK) -> Void
    let onDelete: (ImportedAPK) -> Void
    let onImport: () -> Void

    @State private var searchText = ""
    @State private var selectedCategory: String? = nil

    private let columns = [GridItem(.flexible(), spacing: 12), GridItem(.flexible(), spacing: 12)]

    private var filtered: [ImportedAPK] {
        let query = searchText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !query.isEmpty else { return packages }
        return packages.filter { $0.displayName.localizedCaseInsensitiveContains(query) }
    }

    private var categories: [(String, String, [ImportedAPK])] {
        let definitions: [(String, String, [String])] = [
            ("Games", "gamecontroller.fill", ["game", "racing", "surfer", "hill", "driving", "2048", "pool", "clash", "subway", "angry", "car", "football", "soccer"]),
            ("Entertainment", "play.rectangle.fill", ["video", "music", "player", "stream", "movie", "tv", "media"]),
            ("Social", "person.2.fill", ["chat", "social", "message", "telegram", "discord", "whatsapp", "instagram", "facebook"]),
            ("Utilities", "wrench.and.screwdriver.fill", ["tool", "file", "calculator", "clock", "settings", "browser", "vpn", "store"])
        ]
        var assigned = Set<String>()
        var result: [(String, String, [ImportedAPK])] = definitions.map { name, symbol, terms in
            let matches = packages.filter { item in
                let key = item.packageDirectoryURL.standardizedFileURL.path
                guard !assigned.contains(key) else { return false }
                let title = item.displayName.lowercased()
                let match = terms.contains { title.contains($0) }
                if match { assigned.insert(key) }
                return match
            }
            return (name, symbol, matches)
        }
        let others = packages.filter { !assigned.contains($0.packageDirectoryURL.standardizedFileURL.path) }
        if !others.isEmpty { result.append(("Other", "square.grid.2x2.fill", others)) }
        return result.filter { !$0.2.isEmpty }
    }

    private var suggested: [ImportedAPK] { Array(packages.prefix(4)) }

    var body: some View {
        ZStack {
            LinearGradient(colors: [Color(red: 0.055, green: 0.065, blue: 0.11), Color(red: 0.025, green: 0.03, blue: 0.055)], startPoint: .topLeading, endPoint: .bottomTrailing)
                .ignoresSafeArea()
            ScrollView(showsIndicators: false) {
                VStack(alignment: .leading, spacing: 22) {
                    HStack(alignment: .center) {
                        VStack(alignment: .leading, spacing: 3) {
                            Text("App Library")
                                .font(.system(size: 32, weight: .bold, design: .rounded))
                                .foregroundStyle(.white)
                            Text("\(packages.count) apps")
                                .font(.system(size: 13, weight: .medium))
                                .foregroundStyle(.white.opacity(0.58))
                        }
                        Spacer()
                        Button(action: onImport) {
                            Image(systemName: "plus")
                                .font(.system(size: 17, weight: .semibold))
                                .foregroundStyle(.white)
                                .frame(width: 42, height: 42)
                                .background(.ultraThinMaterial, in: Circle())
                                .overlay(Circle().stroke(.white.opacity(0.16), lineWidth: 1))
                        }
                        .accessibilityLabel("Import app")
                    }

                    HStack(spacing: 9) {
                        Image(systemName: "magnifyingglass").foregroundStyle(.white.opacity(0.65))
                        TextField("App Library Search", text: $searchText)
                            .font(.system(size: 16))
                            .foregroundStyle(.white)
                            .tint(.white)
                            .autocorrectionDisabled()
                            .textInputAutocapitalization(.never)
                        if !searchText.isEmpty {
                            Button { searchText = ""; selectedCategory = nil } label: {
                                Image(systemName: "xmark.circle.fill").foregroundStyle(.white.opacity(0.55))
                            }
                        }
                    }
                    .padding(.horizontal, 15)
                    .frame(height: 48)
                    .background(.ultraThinMaterial, in: Capsule())
                    .overlay(Capsule().stroke(.white.opacity(0.13), lineWidth: 1))

                    if searchText.isEmpty && !suggested.isEmpty {
                        VStack(alignment: .leading, spacing: 12) {
                            sectionTitle("SUGGESTIONS", trailing: "Recently added")
                            HStack(spacing: 13) {
                                ForEach(suggested) { item in
                                    Button { onSelect(item) } label: {
                                        VStack(spacing: 7) {
                                            StrideAppLibraryIcon(data: item.iconData, accent: accent, size: 54)
                                            Text(item.displayName)
                                                .font(.system(size: 10, weight: .medium))
                                                .foregroundStyle(.white)
                                                .lineLimit(1)
                                                .frame(maxWidth: .infinity)
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
                            .padding(14)
                            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
                            .overlay(RoundedRectangle(cornerRadius: 24).stroke(.white.opacity(0.10), lineWidth: 1))
                        }
                    }

                    if filtered.isEmpty {
                        VStack(spacing: 10) {
                            Image(systemName: "app.dashed").font(.system(size: 34)).foregroundStyle(.white.opacity(0.55))
                            Text(packages.isEmpty ? "Your App Library is empty" : "No apps found")
                                .font(.system(size: 17, weight: .semibold)).foregroundStyle(.white)
                            Text(packages.isEmpty ? "Apps you import will appear here automatically." : "Try another app name.")
                                .font(.system(size: 13)).foregroundStyle(.white.opacity(0.58))
                            if packages.isEmpty {
                                Button(action: onImport) {
                                    Label("Import App", systemImage: "plus")
                                        .font(.system(size: 14, weight: .semibold))
                                        .padding(.horizontal, 18).padding(.vertical, 11)
                                        .background(accent, in: Capsule()).foregroundStyle(.white)
                                }
                            }
                        }
                        .frame(maxWidth: .infinity).padding(.vertical, 44)
                    } else if !searchText.isEmpty {
                        VStack(alignment: .leading, spacing: 10) {
                            sectionTitle("SEARCH RESULTS", trailing: "\(filtered.count)")
                            appGrid(filtered)
                        }
                    } else {
                        VStack(alignment: .leading, spacing: 12) {
                            sectionTitle("CATEGORIES", trailing: "Automatically organized")
                            LazyVGrid(columns: columns, spacing: 12) {
                                ForEach(categories.indices, id: \.self) { index in
                                    let category = categories[index]
                                    categoryCard(category.0, symbol: category.1, apps: category.2)
                                }
                            }
                        }
                        if let selectedCategory, let category = categories.first(where: { $0.0 == selectedCategory }) {
                            VStack(alignment: .leading, spacing: 12) {
                                HStack {
                                    Text(category.0).font(.system(size: 21, weight: .bold)).foregroundStyle(.white)
                                    Spacer()
                                    Button("Done") { self.selectedCategory = nil }.font(.system(size: 13, weight: .semibold)).foregroundStyle(.white.opacity(0.75))
                                }
                                appGrid(category.2)
                            }
                            .padding(15)
                            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
                        }
                    }
                }
                .padding(.horizontal, 18)
                .padding(.top, 14)
                .padding(.bottom, 32)
            }
        }
        .preferredColorScheme(.dark)
    }

    private func sectionTitle(_ title: String, trailing: String) -> some View {
        HStack {
            Text(title).font(.system(size: 10, weight: .heavy)).tracking(1.2).foregroundStyle(.white.opacity(0.58))
            Spacer()
            Text(trailing).font(.system(size: 10, weight: .medium)).foregroundStyle(.white.opacity(0.42))
        }
    }

    private func categoryCard(_ title: String, symbol: String, apps: [ImportedAPK]) -> some View {
        Button { selectedCategory = selectedCategory == title ? nil : title } label: {
            VStack(alignment: .leading, spacing: 11) {
                HStack {
                    Image(systemName: symbol).font(.system(size: 12, weight: .semibold)).foregroundStyle(.white.opacity(0.8))
                    Spacer()
                    Text("\(apps.count)").font(.system(size: 10, weight: .semibold)).foregroundStyle(.white.opacity(0.55))
                }
                LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 5), count: 3), spacing: 7) {
                    ForEach(Array(apps.prefix(6))) { item in
                        StrideAppLibraryIcon(data: item.iconData, accent: accent, size: 34)
                    }
                    ForEach(0..<max(0, 6 - min(6, apps.count)), id: \.self) { _ in
                        RoundedRectangle(cornerRadius: 8).fill(.white.opacity(0.07)).frame(width: 34, height: 34)
                    }
                }
                Text(title).font(.system(size: 14, weight: .semibold)).foregroundStyle(.white)
                    .lineLimit(1)
            }
            .padding(12)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 20, style: .continuous))
            .overlay(RoundedRectangle(cornerRadius: 20).stroke(selectedCategory == title ? accent.opacity(0.7) : .white.opacity(0.10), lineWidth: 1))
        }
        .buttonStyle(.plain)
    }

    private func appGrid(_ items: [ImportedAPK]) -> some View {
        LazyVGrid(columns: Array(repeating: GridItem(.flexible(), spacing: 10), count: 3), spacing: 16) {
            ForEach(items) { item in
                Button { onSelect(item) } label: {
                    VStack(spacing: 6) {
                        StrideAppLibraryIcon(data: item.iconData, accent: accent, size: 54)
                        Text(item.displayName).font(.system(size: 10, weight: .medium)).foregroundStyle(.white).lineLimit(1)
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
                    .padding(size * 0.04)
                    .background(Color.white.opacity(0.035))
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
