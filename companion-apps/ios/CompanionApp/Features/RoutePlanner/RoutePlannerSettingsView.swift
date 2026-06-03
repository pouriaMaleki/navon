import SwiftUI

struct RoutePlannerSettingsView: View {
    @EnvironmentObject private var appModel: AppModel
    @EnvironmentObject private var routeHistoryService: RouteHistoryService

    var body: some View {
        Form {
            Section(T.string("settings.routePlanner.defaults.section")) {
                Picker("Default route source", selection: Binding(
                    get: { routeHistoryService.routePlannerPreferences.defaultSourceMode },
                    set: { newValue in
                        var preferences = routeHistoryService.routePlannerPreferences
                        preferences.defaultSourceMode = newValue
                        routeHistoryService.routePlannerPreferences = preferences
                        appModel.currentSourceMode = newValue
                    }
                )) {
                    ForEach(HslAvailabilityService.sourceModeOptions(settings: appModel.settings, request: appModel.routeRequest)) { mode in
                        Text(mode.displayName).tag(mode)
                    }
                }

                Picker("Suggestions", selection: Binding(
                    get: { routeHistoryService.routePlannerPreferences.suggestionMode },
                    set: { newValue in
                        var preferences = routeHistoryService.routePlannerPreferences
                        preferences.suggestionMode = newValue
                        routeHistoryService.routePlannerPreferences = preferences
                    }
                )) {
                    ForEach(RouteSuggestionMode.allCases) { mode in
                        Text(mode.displayName).tag(mode)
                    }
                }

                Picker("Start behavior", selection: Binding(
                    get: { routeHistoryService.routePlannerPreferences.startBehavior },
                    set: { newValue in
                        var preferences = routeHistoryService.routePlannerPreferences
                        preferences.startBehavior = newValue
                        routeHistoryService.routePlannerPreferences = preferences
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
                        Text(T.string("settings.routePlanner.cyclingSpeed"))
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
                Text(T.string("settings.routePlanner.riding.section"))
            } footer: {
                Text(T.string("settings.routePlanner.riding.footnote"))
                    .font(.footnote)
            }

            Section {
                Toggle("Prefer live HSL routing", isOn: $appModel.settings.preferLiveHslRouting)
                SecureField("Digitransit subscription key", text: $appModel.settings.hslSubscriptionKey)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
            } header: {
                Text(T.string("settings.routePlanner.hsl.section"))
            } footer: {
                VStack(alignment: .leading, spacing: 6) {
                    Text(T.string("settings.routePlanner.hsl.description"))
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
