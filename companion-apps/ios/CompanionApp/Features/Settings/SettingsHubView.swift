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
                    LocaleSettingsSection()
                }
                Section {
                    NavigationLink(T.string("settings.hub.routes")) {
                        RoutesSettingsView()
                    }
                    NavigationLink(T.string("settings.hub.device")) {
                        DeviceSettingsView()
                    }
                    NavigationLink(T.string("settings.hub.routePlanner")) {
                        RoutePlannerSettingsView()
                    }
                    NavigationLink(T.string("settings.hub.importDiagnostics")) {
                        ImportDiagnosticsView()
                    }
                }
            }
            .navigationTitle(T.string("settings.hub.title"))
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button(T.string("common.close")) { dismiss() }
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
                    Text(T.string("settings.activity.keepScreenOn.title"))
                    Text(T.string("settings.activity.keepScreenOn.subtitle"))
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
                    Text(T.string("settings.activity.allowBackgroundGps.title"))
                    Text(T.string("settings.activity.allowBackgroundGps.subtitle"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if appModel.locationManualSettingsHint {
                        Text(T.string("home.iosLocationHint"))
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
                    Text(T.string("settings.activity.audioCues.title"))
                    Text(gpsOn
                         ? T.string("settings.activity.audioCues.subtitle")
                         : "Requires GPS in background. Turn that on first.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .disabled(!gpsOn)
            .accessibilityIdentifier("setting-audioCuesEnabled")

            Toggle(isOn: Binding(
                get: { appModel.settings.audioCuesOnlyInBackground },
                set: { newValue in
                    appModel.settings.audioCuesOnlyInBackground = newValue
                    appModel.persistSettings()
                }
            )) {
                VStack(alignment: .leading) {
                    Text(T.string("settings.activity.audioCuesOnlyInBackground.title"))
                    Text(T.string("settings.activity.audioCuesOnlyInBackground.subtitle"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            }
            .disabled(!gpsOn || !appModel.settings.audioCuesEnabled)
            .accessibilityIdentifier("setting-audioCuesOnlyInBackground")

            Toggle(isOn: Binding(
                get: { appModel.settings.liveActivityEnabled },
                set: { newValue in
                    appModel.settings.liveActivityEnabled = newValue
                    appModel.persistSettings()
                }
            )) {
                VStack(alignment: .leading) {
                    Text(T.string("settings.activity.liveActivity.title"))
                    Text(gpsOn
                         ? T.string("settings.activity.liveActivity.subtitle")
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

/// Language + Distance Units pickers. iOS 17 picks them up automatically
/// from system Settings (Per-App Language Preferences) given the
/// CFBundleLocalizations + CFBundleAllowMixedLocalizations entries in
/// Info.plist; this in-app picker writes the same preference.
private struct LocaleSettingsSection: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        let resolvedLocale = T.resolveLocale(appModel.settings.language)
        let hasVoice = SpeechService.hasVoice(forLocale: resolvedLocale.rawValue)
        VStack(alignment: .leading, spacing: 12) {
            Picker(selection: Binding(
                get: { appModel.settings.language },
                set: { newValue in
                    appModel.settings.language = newValue
                    appModel.persistSettings()
                }
            )) {
                Text(T.string("settings.language.system")).tag(AppLanguage.system)
                ForEach(AppLanguage.allCases.filter { $0 != .system }, id: \.self) { lang in
                    // Native name regardless of active locale, matching
                    // iOS Settings convention. SupportedLocale's rawValue
                    // matches AppLanguage's for every concrete case.
                    let native = SupportedLocale(rawValue: lang.rawValue)?.nativeName ?? lang.rawValue
                    Text(native).tag(lang)
                }
            } label: {
                VStack(alignment: .leading) {
                    Text(T.string("settings.language.title"))
                    Text(T.string("settings.language.subtitle"))
                        .font(.caption)
                        .foregroundStyle(.secondary)
                    if !hasVoice {
                        Text(T.string(
                            "settings.language.noVoiceFallback",
                            ["language": .string(resolvedLocale.nativeName)]
                        ))
                        .font(.caption2)
                        .foregroundStyle(.orange)
                        .accessibilityIdentifier("setting-language-no-voice-hint")
                    }
                }
            }
            .accessibilityIdentifier("setting-language")

            Picker(selection: Binding(
                get: { appModel.settings.distanceUnit },
                set: { newValue in
                    appModel.settings.distanceUnit = newValue
                    appModel.persistSettings()
                }
            )) {
                Text(T.string("settings.distanceUnit.system")).tag(DistanceUnitPref.system)
                Text(T.string("settings.distanceUnit.metric")).tag(DistanceUnitPref.metric)
                Text(T.string("settings.distanceUnit.imperial")).tag(DistanceUnitPref.imperial)
            } label: {
                Text(T.string("settings.distanceUnit.title"))
            }
            .accessibilityIdentifier("setting-distanceUnit")
        }
    }
}
