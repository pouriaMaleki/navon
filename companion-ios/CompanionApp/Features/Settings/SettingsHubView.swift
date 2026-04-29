import SwiftUI

struct SettingsHubView: View {
    @EnvironmentObject private var appModel: AppModel
    @Environment(\.dismiss) private var dismiss

    var body: some View {
        NavigationStack {
            List {
                // UX spec lines 128-145: prevent screen off, allow GPS in
                // background, audio cues, and live activity must appear at
                // the TOP of the settings page in this exact order.
                Section {
                    ActivitySettingsSection()
                }
                Section {
                    NavigationLink("Routes") {
                        RoutesSettingsView()
                    }
                    NavigationLink("Device") {
                        DeviceSettingsView()
                    }
                    NavigationLink("Route Planner") {
                        RoutePlannerSettingsView()
                    }
                    NavigationLink("Import Diagnostics") {
                        ImportDiagnosticsView()
                    }
                }
            }
            .navigationTitle("Settings")
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Done") { dismiss() }
                }
            }
        }
    }
}

private struct ActivitySettingsSection: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        let gpsOn = appModel.settings.allowBackgroundGps
        VStack(alignment: .leading, spacing: 12) {
            Toggle(isOn: Binding(
                get: { appModel.settings.keepScreenOn },
                set: { newValue in
                    appModel.settings.keepScreenOn = newValue
                    appModel.persistSettings()
                }
            )) {
                VStack(alignment: .leading) {
                    Text("Prevent screen from turning off")
                    Text("Keeps the display awake while a route is active.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .accessibilityIdentifier("setting-keepScreenOn")

            Toggle(isOn: Binding(
                get: { appModel.settings.allowBackgroundGps },
                set: { newValue in
                    appModel.settings.allowBackgroundGps = newValue
                    appModel.persistSettings()
                    if newValue {
                        appModel.requestAlwaysLocationAuthorization()
                    }
                }
            )) {
                VStack(alignment: .leading) {
                    Text("Allow GPS in background")
                    Text("Required for audio cues and lock-screen route status. Pick \"Always\" when prompted.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if appModel.locationManualSettingsHint {
                        Text("Open iOS Settings → Privacy → Location to switch this app to \"Always\".")
                            .font(.caption2)
                            .foregroundStyle(.orange)
                    }
                }
            }
            .accessibilityIdentifier("setting-allowBackgroundGps")

            Toggle(isOn: Binding(
                get: { appModel.settings.audioCuesEnabled },
                set: { newValue in
                    appModel.settings.audioCuesEnabled = newValue
                    appModel.persistSettings()
                }
            )) {
                VStack(alignment: .leading) {
                    Text("Audio cues")
                    Text(gpsOn
                         ? "Spoken turn-by-turn while you ride."
                         : "Requires GPS in background. Turn that on first.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .disabled(!gpsOn)
            .accessibilityIdentifier("setting-audioCuesEnabled")

            Toggle(isOn: Binding(
                get: { appModel.settings.liveActivityEnabled },
                set: { newValue in
                    appModel.settings.liveActivityEnabled = newValue
                    appModel.persistSettings()
                }
            )) {
                VStack(alignment: .leading) {
                    Text("Lock-screen live activity")
                    Text(gpsOn
                         ? "Show route status and next turn on your lock screen."
                         : "Requires GPS in background. Turn that on first.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .disabled(!gpsOn)
            .accessibilityIdentifier("setting-liveActivityEnabled")
        }
        .accessibilityIdentifier("activity-settings")
    }
}
