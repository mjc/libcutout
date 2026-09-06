import CutoutMobileFFI
import Foundation
import SwiftUI
#if canImport(UIKit) && os(iOS)
import UIKit
#endif
#if canImport(AppKit)
import AppKit
#endif

/// A small, reusable control surface for Ride and Map. It renders metadata only;
/// neither artwork bytes nor an audio stream cross the app boundary.
public struct MusicCompactPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onDismiss: () -> Void
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool
    @State private var isExpanded = false
    @State private var accessibilityAnnouncementTracker = MusicAccessibilityAnnouncementTracker()

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto = .appleMusic,
        historyPolicy: MobileMusicHistoryPolicyDto = .disabled,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onDismiss: @escaping () -> Void = {},
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void = { _ in },
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool = { _ in false }
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.selectedProvider = selectedProvider
        self.historyPolicy = historyPolicy
        self.onCommand = onCommand
        self.onDismiss = onDismiss
        self.onSelectProvider = onSelectProvider
        self.onSetHistoryPolicy = onSetHistoryPolicy
    }

    public var body: some View {
        HStack(spacing: 12) {
            artworkView
            VStack(alignment: .leading, spacing: 2) {
                Text(nowPlaying.title)
                    .lineLimit(1)
                    .font(.subheadline.weight(.semibold))
                Text(nowPlaying.statusText ?? nowPlaying.artist)
                    .lineLimit(1)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 4)
            if nowPlaying.isCommandAvailable(.previous) {
                Button { onCommand(.previous) } label: {
                    Image(systemName: "backward.fill")
                }
                .accessibilityLabel(pevLocalizedText("music.previous"))
            }
            if let command = nowPlaying.playPauseCommand {
                Button { onCommand(command) } label: {
                    Image(systemName: command == .pause ? "pause.fill" : "play.fill")
                }
                .accessibilityLabel(
                    pevLocalizedText(command == .pause ? "music.pause" : "music.play")
                )
            }
            if nowPlaying.isCommandAvailable(.next) {
                Button { onCommand(.next) } label: {
                    Image(systemName: "forward.fill")
                }
                .accessibilityLabel(pevLocalizedText("music.next"))
            }
            if nowPlaying.isCommandAvailable(.openProvider) {
                Button { onCommand(.openProvider) } label: {
                    Image(systemName: "arrow.up.forward.app")
                }
                .accessibilityLabel(pevLocalizedText("music.open_provider"))
            }
            Button { isExpanded = true } label: {
                Image(systemName: "ellipsis.circle")
            }
            .accessibilityLabel(pevLocalizedText("music.expand"))
            Button(action: onDismiss) {
                Image(systemName: "xmark")
            }
            .accessibilityLabel(pevLocalizedText("music.hide"))
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 16))
        .accessibilityElement(children: .contain)
        .accessibilityLabel(nowPlaying.accessibilitySummary)
        .onChange(of: nowPlaying) { _, nowPlaying in
            guard let announcement = accessibilityAnnouncementTracker.next(for: nowPlaying) else {
                return
            }
            AccessibilityNotification.Announcement(announcement).post()
        }
        .sheet(isPresented: $isExpanded) {
            MusicExpandedPlayer(
                nowPlaying: nowPlaying,
                timeline: timeline,
                selectedProvider: selectedProvider,
                historyPolicy: historyPolicy,
                onSelectProvider: onSelectProvider,
                onSetHistoryPolicy: onSetHistoryPolicy
            )
        }
    }

    @ViewBuilder
    private var artworkView: some View {
#if canImport(UIKit) && os(iOS)
        if let data = nowPlaying.artwork?.data, let image = UIImage(data: data) {
            Image(uiImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 34, height: 34)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            Image(systemName: "music.note")
                .accessibilityHidden(true)
        }
#elseif canImport(AppKit)
        if let data = nowPlaying.artwork?.data, let image = NSImage(data: data) {
            Image(nsImage: image)
                .resizable()
                .scaledToFill()
                .frame(width: 34, height: 34)
                .clipShape(RoundedRectangle(cornerRadius: 6))
                .accessibilityLabel(nowPlaying.artworkAccessibilityLabel)
        } else {
            Image(systemName: "music.note")
                .accessibilityHidden(true)
        }
#else
        Image(systemName: "music.note")
            .accessibilityHidden(true)
#endif
    }
}

private struct MusicTimelineRow: View {
    let event: MobileMusicRideEventDto

    var body: some View {
        HStack(spacing: 12) {
            VStack(alignment: .leading, spacing: 2) {
                Text(event.timelineItemTitle)
                    .lineLimit(1)
                Text("\(event.provider.title) · \(event.kind.timelineTitle)")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer(minLength: 8)
            Text(
                Date(timeIntervalSince1970: Double(event.wallClockAtMs) / 1_000),
                style: .time
            )
            .font(.caption2.monospacedDigit())
            .foregroundStyle(.secondary)
        }
        .accessibilityElement(children: .combine)
    }
}

public struct MusicTimelineRows: View {
    public let events: [MobileMusicRideEventDto]

    public init(events: [MobileMusicRideEventDto]) {
        self.events = events
    }

    public var body: some View {
        ForEach(events, id: \.timelineID) { event in
            MusicTimelineRow(event: event)
        }
    }
}

public struct MusicExpandedPlayer: View {
    public let nowPlaying: MusicNowPlaying
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool
    @Environment(\.dismiss) private var dismiss
    @State private var selectedPolicy: MobileMusicHistoryPolicyDto

    public init(
        nowPlaying: MusicNowPlaying,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto,
        historyPolicy: MobileMusicHistoryPolicyDto,
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void,
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool
    ) {
        self.nowPlaying = nowPlaying
        self.timeline = timeline
        self.selectedProvider = selectedProvider
        self.historyPolicy = historyPolicy
        self.onSelectProvider = onSelectProvider
        self.onSetHistoryPolicy = onSetHistoryPolicy
        _selectedPolicy = State(initialValue: historyPolicy)
    }

    public var body: some View {
        NavigationStack {
            Form {
                Section {
                    Text(nowPlaying.title)
                        .font(.headline)
                    Text(nowPlaying.artist)
                        .foregroundStyle(.secondary)
                    if let status = nowPlaying.statusText {
                        Text(status)
                            .foregroundStyle(.secondary)
                    }
                } header: {
                    Text(nowPlaying.providerName)
                }

                if timeline.isEmpty == false {
                    Section {
                        MusicTimelineRows(events: timeline)
                    } header: {
                        Text(pevLocalizedText("music.timeline.title"))
                    }
                }

                Section {
                    Picker(
                        pevLocalizedText("music.provider.select"),
                        selection: Binding(
                            get: { selectedProvider },
                            set: { provider in onSelectProvider(provider) }
                        )
                    ) {
                        ForEach(MobileMusicProviderDto.allCases, id: \.self) { provider in
                            Text(provider.title).tag(provider)
                        }
                    }
                } header: {
                    Text(pevLocalizedText("music.provider.select"))
                }

                Section {
                    Picker(pevLocalizedText("music.history.title"), selection: $selectedPolicy) {
                        ForEach(MobileMusicHistoryPolicyDto.allCases, id: \.self) { policy in
                            Text(policy.title)
                                .tag(policy)
                        }
                    }
                    Text(selectedPolicy.explanation)
                        .font(.footnote)
                        .foregroundStyle(.secondary)
                } header: {
                    Text(pevLocalizedText("music.history.title"))
                }
            }
            .navigationTitle(pevLocalizedText("music.expand"))
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button(pevLocalizedText("music.done")) { dismiss() }
                }
            }
            .onChange(of: selectedPolicy) { _, policy in
                if !onSetHistoryPolicy(policy) {
                    selectedPolicy = historyPolicy
                }
            }
            .onChange(of: historyPolicy) { _, policy in
                selectedPolicy = policy
            }
        }
    }
}

/// Shared Ride/Map composition for the compact player.
public struct MusicCompactPlayerInset: ViewModifier {
    public let nowPlaying: MusicNowPlaying?
    public let timeline: [MobileMusicRideEventDto]
    public let selectedProvider: MobileMusicProviderDto
    public let isHidden: Bool
    public let historyPolicy: MobileMusicHistoryPolicyDto
    public let onCommand: (MobileMusicCommandDto) -> Void
    public let onConnect: () -> Void
    public let onDismiss: () -> Void
    public let onRestore: () -> Void
    public let onSelectProvider: (MobileMusicProviderDto) -> Void
    public let onSetHistoryPolicy: (MobileMusicHistoryPolicyDto) -> Bool

    public func body(content: Content) -> some View {
        content.safeAreaInset(edge: .bottom, spacing: 8) {
            if let nowPlaying {
                MusicCompactPlayer(
                    nowPlaying: nowPlaying,
                    timeline: timeline,
                    selectedProvider: selectedProvider,
                    historyPolicy: historyPolicy,
                    onCommand: onCommand,
                    onDismiss: onDismiss,
                    onSelectProvider: onSelectProvider,
                    onSetHistoryPolicy: onSetHistoryPolicy
                )
                .padding(.horizontal, 12)
            } else if isHidden {
                Button(action: onRestore) {
                    Label(
                        pevLocalizedText("music.restore"),
                        systemImage: "music.note"
                    )
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("music.restore")
            } else {
                Button(action: onConnect) {
                    Label(
                        pevLocalizedText("music.connect"),
                        systemImage: "music.note"
                    )
                }
                .buttonStyle(.bordered)
                .accessibilityIdentifier("music.connect")
            }
        }
    }
}

public extension View {
    func musicCompactPlayer(
        nowPlaying: MusicNowPlaying?,
        timeline: [MobileMusicRideEventDto] = [],
        selectedProvider: MobileMusicProviderDto,
        isHidden: Bool,
        historyPolicy: MobileMusicHistoryPolicyDto,
        onCommand: @escaping (MobileMusicCommandDto) -> Void,
        onConnect: @escaping () -> Void,
        onDismiss: @escaping () -> Void,
        onRestore: @escaping () -> Void,
        onSelectProvider: @escaping (MobileMusicProviderDto) -> Void,
        onSetHistoryPolicy: @escaping (MobileMusicHistoryPolicyDto) -> Bool
    ) -> some View {
        modifier(MusicCompactPlayerInset(
            nowPlaying: nowPlaying,
            timeline: timeline,
            selectedProvider: selectedProvider,
            isHidden: isHidden,
            historyPolicy: historyPolicy,
            onCommand: onCommand,
            onConnect: onConnect,
            onDismiss: onDismiss,
            onRestore: onRestore,
            onSelectProvider: onSelectProvider,
            onSetHistoryPolicy: onSetHistoryPolicy
        ))
    }
}
