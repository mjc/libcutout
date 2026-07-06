import CutoutMobile
import SwiftUI

struct LiveActivityMockupView: View {
    let screen: MockupScreen

    @State private var selectedFixtureKind = LiveActivityRideFixtureKind.demo

    private let fixtures = LiveActivityRideFixtureMatrix.v1.fixtures

    private var selectedFixture: LiveActivityRideFixture {
        fixtures.first { $0.kind == selectedFixtureKind } ?? fixtures[0]
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 20) {
                header
                fixturePicker
                VStack(alignment: .leading, spacing: 16) {
                    LiveActivityPresentationCard(
                        title: "Compact",
                        subtitle: "Glanceable island treatment",
                        style: .compact,
                        snapshot: selectedFixture.snapshot
                    )
                    LiveActivityPresentationCard(
                        title: "Expanded",
                        subtitle: "Dynamic Island detail state",
                        style: .expanded,
                        snapshot: selectedFixture.snapshot
                    )
                    LiveActivityPresentationCard(
                        title: "Lock Screen",
                        subtitle: "Full-width lock-screen card",
                        style: .lockScreen,
                        snapshot: selectedFixture.snapshot
                    )
                }
                fixtureLegend
            }
            .padding(24)
        }
        .background(LiveActivityMockupPalette.background.ignoresSafeArea())
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(screen.title)
                .font(.largeTitle.bold())
                .foregroundStyle(LiveActivityMockupPalette.primaryText)
            Text(screen.subtitle)
                .font(.callout)
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
            HStack(spacing: 8) {
                Label("Fixture-only", systemImage: "cube.transparent")
                Label("Matrix-driven", systemImage: "rectangle.3.group")
            }
            .font(.caption.weight(.semibold))
            .foregroundStyle(LiveActivityMockupPalette.secondaryText)
        }
    }

    private var fixturePicker: some View {
        Picker("Fixture", selection: $selectedFixtureKind) {
            ForEach(fixtures, id: \.kind) { fixture in
                Text(fixture.kind.displayTitle).tag(fixture.kind)
            }
        }
        .pickerStyle(.menu)
        .foregroundStyle(LiveActivityMockupPalette.primaryText)
    }

    private var fixtureLegend: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Fixture states")
                .font(.headline)
                .foregroundStyle(LiveActivityMockupPalette.primaryText)
            ForEach(fixtures, id: \.kind) { fixture in
                HStack(spacing: 12) {
                    Circle()
                        .fill(fixture.kind == selectedFixtureKind ? LiveActivityMockupPalette.accent : LiveActivityMockupPalette.muted)
                        .frame(width: 8, height: 8)
                    Text(fixture.kind.displayTitle)
                        .foregroundStyle(LiveActivityMockupPalette.primaryText)
                    Spacer()
                    Text(fixture.snapshot.connectionState.rawValue)
                        .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                }
                .font(.callout)
            }
        }
        .padding(.top, 4)
    }
}

private enum LiveActivityPresentationStyle {
    case compact
    case expanded
    case lockScreen
}

private struct LiveActivityPresentationCard: View {
    let title: String
    let subtitle: String
    let style: LiveActivityPresentationStyle
    let snapshot: LiveActivityRideSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            header
            switch style {
            case .compact:
                compactBody
            case .expanded:
                expandedBody
            case .lockScreen:
                lockScreenBody
            }
        }
        .padding(style == .compact ? 18 : 24)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(LiveActivityMockupPalette.cardBackground)
        .overlay(
            RoundedRectangle(cornerRadius: 20, style: .continuous)
                .stroke(LiveActivityMockupPalette.border, lineWidth: 1)
        )
        .clipShape(RoundedRectangle(cornerRadius: 20, style: .continuous))
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.headline.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.primaryText)
            Text(subtitle)
                .font(.caption)
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
        }
    }

    private var compactBody: some View {
        HStack(alignment: .firstTextBaseline, spacing: 20) {
            VStack(alignment: .leading, spacing: 6) {
                Text(snapshot.identity.displayLabel)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                Text(snapshot.speed.value)
                    .font(.system(size: 34, weight: .bold, design: .rounded))
                    .foregroundStyle(LiveActivityMockupPalette.primaryText)
                Text(snapshot.connectionState.rawValue)
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(LiveActivityMockupPalette.secondaryText)
            }

            Spacer()

            VStack(alignment: .trailing, spacing: 8) {
                liveMetric(title: snapshot.battery.label, value: snapshot.battery.value)
                liveMetric(title: snapshot.headroom.label, value: snapshot.headroom.value)
            }
        }
    }

    private var expandedBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(snapshot.identity.displayLabel)
                .font(.callout.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)

            Text(snapshot.speed.value)
                .font(.system(size: 52, weight: .bold, design: .rounded))
                .foregroundStyle(LiveActivityMockupPalette.primaryText)

            LazyVGrid(columns: [
                GridItem(.flexible(), spacing: 12),
                GridItem(.flexible(), spacing: 12),
            ], spacing: 12) {
                ForEach(snapshot.visibleValues, id: \.label) { value in
                    LiveActivityValueTile(value: value)
                }
            }
        }
    }

    private var lockScreenBody: some View {
        VStack(alignment: .leading, spacing: 16) {
            HStack(alignment: .top) {
                VStack(alignment: .leading, spacing: 4) {
                    Text(snapshot.identity.displayLabel)
                        .font(.headline.weight(.semibold))
                        .foregroundStyle(LiveActivityMockupPalette.primaryText)
                    Text(snapshot.connectionState.rawValue)
                        .font(.caption)
                        .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                }
                Spacer()
                Text(snapshot.speed.value)
                    .font(.system(size: 48, weight: .bold, design: .rounded))
                    .foregroundStyle(LiveActivityMockupPalette.primaryText)
            }

            HStack(alignment: .top, spacing: 12) {
                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(title: snapshot.battery.label, value: snapshot.battery.value)
                    liveMetric(title: snapshot.packVoltage.label, value: snapshot.packVoltage.value)
                    liveMetric(title: snapshot.pwm.label, value: snapshot.pwm.value)
                }

                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(title: snapshot.mode.label, value: snapshot.mode.value)
                    liveMetric(title: snapshot.duration.label, value: snapshot.duration.value)
                    liveMetric(title: snapshot.distance.label, value: snapshot.distance.value)
                }

                VStack(alignment: .leading, spacing: 10) {
                    liveMetric(title: snapshot.headroom.label, value: snapshot.headroom.value)
                    liveMetric(title: snapshot.beeps.label, value: snapshot.beeps.value)
                    liveMetric(title: snapshot.temperature.label, value: snapshot.temperature.value)
                }
            }
        }
    }

    private func liveMetric(title: String, value: String) -> some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(title)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
            Text(value)
                .font(.callout.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.primaryText)
                .lineLimit(1)
        }
    }
}

private struct LiveActivityValueTile: View {
    let value: LiveActivityRideValue

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(value.label)
                .font(.caption2.weight(.semibold))
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
            HStack(alignment: .firstTextBaseline, spacing: 4) {
                Text(value.value)
                    .font(.headline.weight(.semibold))
                    .foregroundStyle(LiveActivityMockupPalette.primaryText)
                if let unit = value.unit {
                    Text(unit)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(LiveActivityMockupPalette.secondaryText)
                }
            }
            Text(value.state.rawValue)
                .font(.caption2)
                .foregroundStyle(LiveActivityMockupPalette.secondaryText)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(LiveActivityMockupPalette.tileBackground)
        .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

private enum LiveActivityMockupPalette {
    static let background = Color(red: 0.06, green: 0.08, blue: 0.12)
    static let cardBackground = Color(red: 0.10, green: 0.12, blue: 0.17)
    static let tileBackground = Color(red: 0.14, green: 0.17, blue: 0.23)
    static let border = Color.white.opacity(0.10)
    static let accent = Color(red: 0.26, green: 0.70, blue: 0.96)
    static let muted = Color.white.opacity(0.25)
    static let primaryText = Color.white
    static let secondaryText = Color.white.opacity(0.72)
}

private extension LiveActivityRideFixtureKind {
    var displayTitle: String {
        switch self {
        case .demo:
            "Demo"
        case .populated:
            "Populated"
        case .partial:
            "Partial"
        case .waitingForFirstTelemetry:
            "Waiting"
        case .stale:
            "Stale"
        case .disconnected:
            "Disconnected"
        case .parked:
            "Parked"
        }
    }
}
