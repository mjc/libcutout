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
