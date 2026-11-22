import Foundation

enum StrideExecutionMode: String, CaseIterable, Identifiable {
    case jitless
    case jitlessP12
    case externalJIT

    var id: String { rawValue }

    var title: String {
        switch self {
        case .jitless: return "Fully JIT-less"
        case .jitlessP12: return "JIT-less + P12 identity"
        case .externalJIT: return "External JIT"
        }
    }

    var shortTitle: String {
        switch self {
        case .jitless: return "JIT-less"
        case .jitlessP12: return "JIT-less + P12"
        case .externalJIT: return "External JIT"
        }
    }

    var detail: String {
        switch self {
        case .jitless:
            return "Uses Stride's managed interpreter path without requiring a P12 certificate or external JIT."
        case .jitlessP12:
            return "Requires a valid P12 identity stored in Keychain. P12 is separate from JIT and does not currently enable executable signing."
        case .externalJIT:
            return "Reserved for an authorized external JIT provider. Launch remains blocked until a supported provider and authorization adapter are available."
        }
    }
}

enum StrideExecutionPreferences {
    static let defaultModeKey = "stride.execution.defaultMode"

    static var defaultMode: StrideExecutionMode {
        StrideExecutionMode(rawValue: UserDefaults.standard.string(forKey: defaultModeKey) ?? "") ?? .jitless
    }

    static func overrideKey(for url: URL) -> String {
        "stride.execution.override.\(url.standardizedFileURL.path)"
    }

    static func override(for url: URL) -> StrideExecutionMode? {
        guard let raw = UserDefaults.standard.string(forKey: overrideKey(for: url)) else { return nil }
        return StrideExecutionMode(rawValue: raw)
    }

    static func mode(for url: URL) -> StrideExecutionMode {
        override(for: url) ?? defaultMode
    }

    static func setOverride(_ mode: StrideExecutionMode, for url: URL) {
        UserDefaults.standard.set(mode.rawValue, forKey: overrideKey(for: url))
    }

    static func clearOverride(for url: URL) {
        UserDefaults.standard.removeObject(forKey: overrideKey(for: url))
    }
}
