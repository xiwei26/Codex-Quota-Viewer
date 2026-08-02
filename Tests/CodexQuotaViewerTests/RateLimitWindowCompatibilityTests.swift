import Foundation
import Testing

@testable import CodexQuotaViewer

@Test
func rateLimitSnapshotAcceptsArbitraryFlatWindows() throws {
    let data = Data(
        """
        {
          "limitId": "codex",
          "windows": [
            { "label": "monthly", "remainingPercent": 91 },
            { "windowDurationMins": 1440, "usedPercent": 35 },
            { "label": "burst", "remainingPercent": 64 }
          ]
        }
        """.utf8
    )

    let snapshot = try JSONDecoder().decode(RateLimitSnapshot.self, from: data)
    let windows = quotaDisplayWindows(from: snapshot)

    #expect(windows.count == 3)
    #expect(windows.map(\.label) == ["1d", "monthly", "burst"])
    #expect(windows.map(\.window.remainingPercent) == [65, 91, 64])
}

@Test
func quotaOverviewUsesReturnedWindowLabelsAndPreservesAdditionalWindows() {
    withExclusiveAppLocalization {
        AppLocalization.setPreferredLanguage(.en, preferredLanguages: ["en-US"])
        let snapshot = CodexSnapshot(
            account: CodexAccount(type: "chatgpt", email: "user@example.com", planType: "plus"),
            rateLimits: RateLimitSnapshot(
                limitId: "codex",
                limitName: nil,
                primary: nil,
                secondary: nil,
                planType: "plus",
                windows: [
                    RateLimitWindow(usedPercent: 20, windowDurationMins: 720, resetsAt: 1_800_000_360),
                    RateLimitWindow(usedPercent: 40, windowDurationMins: 1_440, resetsAt: 1_800_086_400),
                    RateLimitWindow(usedPercent: 30, windowDurationMins: nil, resetsAt: nil, label: "monthly"),
                ]
            ),
            fetchedAt: Date(timeIntervalSince1970: 1_800_000_000)
        )
        let profile = makeTestProviderProfile(
            id: "dynamic-windows",
            displayName: "user@example.com",
            authMode: .chatgpt,
            snapshot: snapshot
        )

        let row = quotaOverviewRowQuotaTexts(for: profile)

        #expect(row.primaryRemainingText == "12h 80%")
        #expect(row.secondaryRemainingText == "1d 60%")
        #expect(row.additionalSummaryText == "monthly 70%")
        #expect(quotaTileSecondaryText(for: profile) == "1d 60% monthly 70%")
    }
}
