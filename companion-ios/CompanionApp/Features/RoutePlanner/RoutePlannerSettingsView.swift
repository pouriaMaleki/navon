import SwiftUI

struct RoutePlannerSettingsView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        Form {
            Section("Defaults") {
                Picker("Default route source", selection: Binding(
                    get: { appModel.routePlannerPreferences.defaultSourceMode },
                    set: { newValue in
                        var preferences = appModel.routePlannerPreferences
                        preferences.defaultSourceMode = newValue
                        appModel.routePlannerPreferences = preferences
                        appModel.currentSourceMode = newValue
                    }
                )) {
                    ForEach(appModel.sourceModeOptions) { mode in
                        Text(mode.displayName).tag(mode)
                    }
                }

                Picker("Suggestions", selection: Binding(
                    get: { appModel.routePlannerPreferences.suggestionMode },
                    set: { newValue in
                        var preferences = appModel.routePlannerPreferences
                        preferences.suggestionMode = newValue
                        appModel.routePlannerPreferences = preferences
                    }
                )) {
                    ForEach(RouteSuggestionMode.allCases) { mode in
                        Text(mode.displayName).tag(mode)
                    }
                }

                Picker("Start behavior", selection: Binding(
                    get: { appModel.routePlannerPreferences.startBehavior },
                    set: { newValue in
                        var preferences = appModel.routePlannerPreferences
                        preferences.startBehavior = newValue
                        appModel.routePlannerPreferences = preferences
                    }
                )) {
                    ForEach(RouteStartBehavior.allCases) { behavior in
                        Text(behavior.displayName).tag(behavior)
                    }
                }
            }

            Section {
                Stepper(value: $appModel.settings.cyclingSpeedKph, in: 5...50, step: 1) {
                    HStack {
                        Text("Cycling speed")
                        Spacer()
                        Text("\(Int(appModel.settings.cyclingSpeedKph.rounded())) kph")
                            .foregroundStyle(.secondary)
                    }
                }
                Picker("Speed unit", selection: $appModel.settings.speedUnit) {
                    ForEach(SpeedUnit.allCases) { unit in
                        Text(unit.label).tag(unit)
                    }
                }
            } header: {
                Text("Riding")
            } footer: {
                Text("Cycling speed overrides HSL ETAs (Digitransit defaults to a slow bike speed). Speed unit is how the live-speed badge is shown on the map.")
                    .font(.footnote)
            }

            Section {
                Toggle("Prefer live HSL routing", isOn: $appModel.settings.preferLiveHslRouting)
                SecureField("Digitransit subscription key", text: $appModel.settings.hslSubscriptionKey)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            } header: {
                Text("HSL Digitransit")
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text("HSL is the Helsinki Region Transport authority. Their Digitransit API provides high-quality bike routing across the Helsinki metro area. The key is free — sign in at the portal, register an app, and copy the subscription key into the field above. Outside the Helsinki area, leave HSL off and the planner uses OSM routing globally.")
                    Link("Open the Digitransit portal", destination: URL(string: "https://portal-api.digitransit.fi/")!)
                }
                .font(.footnote)
                .padding(.top, 4)
            }
        }
        .navigationTitle(T.string("settings.routePlanner.title"))
        .onChange(of: appModel.settings) { _, _ in
            appModel.persistSettings()
        }
    }
}
