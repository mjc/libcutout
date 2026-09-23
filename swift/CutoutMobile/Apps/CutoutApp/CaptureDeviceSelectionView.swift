import CutoutMobile
import SwiftUI

struct CaptureDeviceSelectionView: View {
    let sections: DevicePickerSections
    let start: (DevicePickerRow, String) -> Bool
    @Environment(\.dismiss) private var dismiss

    private var rows: [DevicePickerRow] {
        sections.supported + sections.probeRecommended + sections.unsupported
    }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    ForEach(rows) { row in
                        NavigationLink {
                            CaptureStartView(row: row, start: start, cancel: { dismiss() })
                        } label: {
                            CaptureDeviceSummary(row: row)
                        }
                        .accessibilityIdentifier("captures.select.\(row.id)")
                    }
                    if rows.isEmpty {
                        ContentUnavailableView(
                            localizedAppText("captures.no_devices"), systemImage: "antenna.radiowaves.left.and.right",
                            description: Text(localizedAppText("captures.no_devices_hint"))
                        )
                    }
                } footer: {
                    Text(localizedAppText("captures.read_only"))
                }
            }
            .scrollContentBackground(.hidden)
            .background(PevColors.pageBackground)
            .navigationTitle(localizedAppText("captures.choose_device"))
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button(localizedAppText("picker.capture_kind.cancel"), role: .cancel) { dismiss() }
                        .accessibilityIdentifier("captures.cancel")
                }
            }
        }
        .tint(PevColors.yellow)
        .accessibilityIdentifier("captures.device-selection")
    }
}

private struct CaptureStartView: View {
    let row: DevicePickerRow
    let start: (DevicePickerRow, String) -> Bool
    let cancel: () -> Void
    @State private var description = ""
    @State private var startFailed = false

    var body: some View {
        Form {
            Section {
                CaptureDeviceSummary(row: row)
            }
            Section {
                TextField(localizedAppText("captures.description"), text: $description, axis: .vertical)
                    .lineLimit(2 ... 5)
                    .accessibilityIdentifier("captures.description")
            } footer: {
                Text(localizedAppText("captures.description_hint"))
            }
            Section {
                Button {
                    startFailed = !start(row, description)
                } label: {
                    Label(localizedAppText("captures.start"), systemImage: "record.circle")
                        .frame(maxWidth: .infinity, minHeight: 44)
                }
                .buttonStyle(.borderedProminent)
                .tint(PevColors.yellow)
                .foregroundStyle(.black)
                .accessibilityIdentifier("captures.start")
                if startFailed {
                    Label(localizedAppText("captures.start_failed"), systemImage: "exclamationmark.triangle")
                        .foregroundStyle(PevColors.warningText)
                        .accessibilityIdentifier("captures.start-error")
                }
            } footer: {
                Text(localizedAppText("captures.read_only"))
            }
        }
        .scrollContentBackground(.hidden)
        .background(PevColors.pageBackground)
        .navigationTitle(localizedAppText("captures.new"))
        .accessibilityIdentifier("captures.setup")
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button(localizedAppText("picker.capture_kind.cancel"), role: .cancel, action: cancel)
                    .accessibilityIdentifier("captures.cancel")
            }
        }
    }
}

private struct CaptureDeviceSummary: View {
    let row: DevicePickerRow

    var body: some View {
        HStack(spacing: 12) {
            DeviceGlyph(row: row)
                .frame(width: 44, height: 44)
                .accessibilityHidden(true)
            VStack(alignment: .leading, spacing: 4) {
                Text(row.title).font(.headline)
                if let identity = row.secondaryIdentity {
                    Text(identity).font(.subheadline).foregroundStyle(.secondary)
                }
                Text(String(row.id.suffix(8)).uppercased())
                    .font(.caption.monospaced()).foregroundStyle(.secondary)
            }
        }
        .padding(.vertical, 6)
        .accessibilityElement(children: .combine)
    }
}
