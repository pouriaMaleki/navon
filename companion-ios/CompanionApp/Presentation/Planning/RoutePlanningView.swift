import SwiftUI
import UniformTypeIdentifiers

struct RoutePlanningView: View {
    @EnvironmentObject private var appModel: AppModel
    @State private var isImportingGpx = false

    private var allowedGpxTypes: [UTType] {
        if let gpx = UTType(filenameExtension: "gpx") {
            return [gpx, .xml]
        }
        return [.xml]
    }

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
                    Text(planningModeTitle)
                    Text(planningModeDetail)
                        .foregroundStyle(.secondary)
                }

                if appModel.selectedProviderID == .gpxImport {
                    Section("Import GPX") {
                        Text("Choose a .gpx file from Files to import a route into the native companion flow.")
                            .foregroundStyle(.secondary)
                        Button("Choose GPX file") {
                            isImportingGpx = true
                        }
                    }
                } else {
                    Section("Origin") {
                        CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.origin.latitude)
                        CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.origin.longitude)
                    }

                    Section("Destination") {
                        CoordinateEditor(title: "Latitude", value: $appModel.routeRequest.destination.latitude)
                        CoordinateEditor(title: "Longitude", value: $appModel.routeRequest.destination.longitude)
                    }
                }

                if let notice = appModel.preview.planningNotice {
                    Section("Last planning notice") {
                        Text(notice)
                            .foregroundStyle(.secondary)
                    }
                }

                if appModel.selectedProviderID != .gpxImport {
                    Section {
                        Button("Plan \(appModel.selectedProviderID.displayName) route") {
                            Task { await appModel.planRoute() }
                        }
                        .disabled(!appModel.selectedProviderCanPlan)
                    }
                }
            }
            .fileImporter(
                isPresented: $isImportingGpx,
                allowedContentTypes: allowedGpxTypes,
                allowsMultipleSelection: false
            ) { result in
                switch result {
                case .success(let urls):
                    guard let url = urls.first else { return }
                    Task { await appModel.importGpxFile(from: url) }
                case .failure(let error):
                    appModel.preview = RoutePreviewModel(
                        alternatives: [],
                        selectedAlternativeID: nil,
                        routeIdentifier: nil,
                        routeRevision: nil,
                        planningNotice: "GPX import failed: \(error.localizedDescription)"
                    )
                }
            }
            .navigationTitle("Plan route")
        }
    }

    private var planningModeTitle: String {
        if appModel.selectedProviderID == .gpxImport {
            return "Native GPX import"
        }
        return appModel.settings.preferLiveHslRouting ? "Live HSL enabled" : "Sample HSL routes"
    }

    private var planningModeDetail: String {
        if appModel.selectedProviderID == .gpxImport {
            return "GPX files are imported through the iOS document picker and normalized into the same route package used by sync and follow logic."
        }
        return appModel.settings.preferLiveHslRouting
            ? "Routes will use Digitransit when a subscription key is configured."
            : "Routes are currently generated from the built-in sample fixtures or sample-backed provider adapters."
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
