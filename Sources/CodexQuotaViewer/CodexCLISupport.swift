import Foundation

enum CodexDesktopInstallation {
    static let chatGPTAppURL = URL(
        fileURLWithPath: "/Applications/ChatGPT.app",
        isDirectory: true
    )
    static let legacyCodexAppURL = URL(
        fileURLWithPath: "/Applications/Codex.app",
        isDirectory: true
    )

    static let chatGPTCLIURL = chatGPTAppURL
        .appendingPathComponent("Contents/Resources/codex", isDirectory: false)
    static let legacyCodexCLIURL = legacyCodexAppURL
        .appendingPathComponent("Contents/Resources/codex", isDirectory: false)

    static let bundledCLIURLs = [chatGPTCLIURL, legacyCodexCLIURL]
    static let appURLs = [chatGPTAppURL, legacyCodexAppURL]
}

struct CodexCLIConfiguration: Equatable {
    let executableURL: URL
    let argumentsPrefix: [String]

    func arguments(appending arguments: [String]) -> [String] {
        argumentsPrefix + arguments
    }
}

func resolveCodexCLIConfiguration(
    preferredExecutableURL: URL? = nil,
    bundledExecutableURLs: [URL] = CodexDesktopInstallation.bundledCLIURLs,
    fileManager: FileManager = .default,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> CodexCLIConfiguration? {
    let candidateURLs = [preferredExecutableURL].compactMap { $0 }
        + bundledExecutableURLs

    var resolvedPaths = Set<String>()

    for candidateURL in candidateURLs {
        guard resolvedPaths.insert(candidateURL.standardizedFileURL.path).inserted else {
            continue
        }
        if fileManager.isExecutableFile(atPath: candidateURL.path) {
            return CodexCLIConfiguration(executableURL: candidateURL, argumentsPrefix: [])
        }
    }

    if let pathExecutableURL = codexExecutableURLInPATH(
        environment: environment,
        fileManager: fileManager
    ) {
        return CodexCLIConfiguration(
            executableURL: pathExecutableURL,
            argumentsPrefix: []
        )
    }

    return nil
}

func resolveCodexDesktopAppURL(
    appURLs: [URL] = CodexDesktopInstallation.appURLs,
    fileManager: FileManager = .default
) -> URL? {
    appURLs.first { fileManager.fileExists(atPath: $0.path) }
}

private func codexExecutableURLInPATH(
    environment: [String: String],
    fileManager: FileManager
) -> URL? {
    guard let rawPATH = environment["PATH"]?
        .trimmingCharacters(in: .whitespacesAndNewlines),
          !rawPATH.isEmpty else {
        return nil
    }

    for directory in rawPATH.split(separator: ":").map(String.init) {
        guard !directory.isEmpty else {
            continue
        }

        let candidateURL = URL(fileURLWithPath: directory, isDirectory: true)
            .appendingPathComponent("codex", isDirectory: false)
        if fileManager.isExecutableFile(atPath: candidateURL.path) {
            return candidateURL
        }
    }

    return nil
}
