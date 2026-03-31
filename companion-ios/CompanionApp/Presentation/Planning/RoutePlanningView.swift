import SwiftUI

struct RoutePlanningView: View {
    @EnvironmentObject private var appModel: AppModel

    var body: some View {
        NavigationStack {
            Form {
                Section("Provider") {
                    Picker("Routing provider", selection: $appModel.selectedProviderID) {
                        ForEach(appModel.providerOptions) { provider in
                            HStack {
                                Text(provider.displayName)
                                if !provider.isAvailableInV1 {
                                    Text(provider.supportsCompanionPreview ? "Sample preview" : "Coming soon")
                                        .foregroundStyle(.secondary)
                                }
                            }
                            .tag(provider)
                        }
                    }
                }

                Section("Planning mode") {
                    Text(appModel.settings.preferLiveHslRouting ? "Live HSL enabled" : "Sample HSL routes")
                    Text(appModel.settings.preferLiveHslRouting ? "Routes will use Digitransit when a subscription key is configured." : "Routes are currently generated from the built-in sample fixtures or sample-backed provider adapters.")
                        .foregroundStyle(.secondary)
                }

                Section("Origin") {
                    CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.origin.latitude)
                    CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.origin.longitude)
                }

                Section("Destination") {
                    CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.destination.latitude)
                    CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.destination.longitude)
                }

                if let notice = appModel.preview.planningNotice {
                    Section("Last planning notice") {
                        Text(notice)
                            .foregroundStyle(.secondary)
                    }
                }

                Section {
                    Button("Plan \(appModel.selectedProviderID.displayName) route") {
                        Task { await appModel.planRoute() }
                    }
                    .disabled(!appModel.selectedProviderCanPlan)
                }
            }
            .navigationTitle("Plan route")
        }
    }
}

private struct CoordinateEditor: View {
    let title: String
    @Binding var value: Double

    var body: some View {
        HStack {
            Text(title)
            Spacer()
            TextField(title, value: $value, format: .number)
                .multilineTextAlignment(.trailing)
                .keyboardType(.decimalPad)
                .frame(width: 140)
        }
    }
}
