import SwiftUI
import MapKit

struct CompanionHomeView: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: HomeViewModel
    @State private var cameraPosition: MapCameraPosition = .region(
        MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: 60.1699, longitude: 24.9384),
            span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
        )
    )

    var body: some View {
        MapReader { proxy in
            ZStack(alignment: .top) {
                mapSurface(proxy: proxy)
                overlayChrome
            }
            .ignoresSafeArea(edges: .bottom)
            .navigationTitle("Companion")
            .navigationBarTitleDisplayMode(.inline)
        }
    }

    @ViewBuilder
    private func mapSurface(proxy: MapProxy) -> some View {
        Map(position: $cameraPosition) {
            UserAnnotation()

            if let destination = viewModel.destinationCoordinate {
                Marker("Destination", coordinate: CLLocationCoordinate2D(latitude: destination.latitude, longitude: destination.longitude))
            }

            ForEach(viewModel.previewAlternatives, id: \.id) { alternative in
                let isSelected = alternative.id == appModel.preview.selectedAlternativeID
                MapPolyline(coordinates: alternative.normalizedPackage.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                    .stroke(isSelected ? .blue : .teal.opacity(0.45), lineWidth: isSelected ? 6 : 4)
            }

            if let active = viewModel.activeRoute {
                MapPolyline(coordinates: active.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                    .stroke(.green, lineWidth: 7)
            }
        }
        .mapControls {
            MapCompass()
            MapUserLocationButton()
        }
        .overlay {
            Color.clear
                .contentShape(Rectangle())
                .gesture(
                    LongPressGesture(minimumDuration: 0.6)
                        .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .local))
                        .onEnded { value in
                            switch value {
                            case .second(true, let drag?):
                                if let coordinate = proxy.convert(drag.location, from: .local) {
                                    viewModel.setDestinationFromMap(CoordinatePoint(latitude: coordinate.latitude, longitude: coordinate.longitude))
                                    centerCamera(on: coordinate)
                                }
                            default:
                                break
                            }
                        }
                )
        }
    }

    private var overlayChrome: some View {
        VStack(spacing: 12) {
            VStack(spacing: 0) {
                searchField
                if viewModel.isSearchOpen {
                    searchPanel
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 8)

            Spacer()

            if let active = viewModel.activeRoute {
                activeGuidanceCard(active)
            } else if !viewModel.previewAlternatives.isEmpty {
                routeSuggestionsCard
            }
        }
    }

    private var searchField: some View {
        HStack(spacing: 12) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            TextField("Where to?", text: Binding(
                get: { viewModel.query },
                set: { newValue in
                    viewModel.openSearch()
                    viewModel.updateQuery(newValue)
                }
            ))
            .textInputAutocapitalization(.words)
            .autocorrectionDisabled()
            .onTapGesture {
                viewModel.openSearch()
            }

            if !viewModel.query.isEmpty {
                Button {
                    viewModel.updateQuery("")
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .foregroundStyle(.secondary)
                }
            }
        }
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var searchPanel: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if viewModel.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ForEach(viewModel.recentItems) { item in
                        Button {
                            viewModel.selectRecent(item)
                        } label: {
                            routeHistoryRow(item)
                        }
                        .buttonStyle(.plain)
                        .onAppear { viewModel.loadMoreRecentsIfNeeded(for: item) }
                        Divider()
                    }
                } else {
                    ForEach(viewModel.visibleSuggestions) { suggestion in
                        Button {
                            centerCamera(on: CLLocationCoordinate2D(latitude: suggestion.coordinate.latitude, longitude: suggestion.coordinate.longitude))
                            viewModel.selectSuggestion(suggestion)
                        } label: {
                            suggestionRow(suggestion)
                        }
                        .buttonStyle(.plain)
                        .onAppear { viewModel.loadMoreSuggestionsIfNeeded(for: suggestion) }
                        Divider()
                    }
                }
            }
        }
        .frame(maxHeight: 320)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private func suggestionRow(_ suggestion: DestinationSearchResult) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: "mappin.and.ellipse")
                .foregroundStyle(.blue)
            VStack(alignment: .leading, spacing: 4) {
                Text(suggestion.title)
                    .font(.headline)
                    .foregroundStyle(.primary)
                if !suggestion.subtitle.isEmpty {
                    Text(suggestion.subtitle)
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                }
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
    }

    private func routeHistoryRow(_ item: RouteHistoryItem) -> some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: item.routePackage == nil ? "clock" : "point.topleft.down.curvedto.point.bottomright.up")
                .foregroundStyle(.teal)
            VStack(alignment: .leading, spacing: 4) {
                Text(item.title)
                    .font(.headline)
                    .foregroundStyle(.primary)
                Text(item.subtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                Text(item.sourceLabel)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Spacer()
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
    }

    private var routeSuggestionsCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            Text("Suggested routes")
                .font(.headline)
            ForEach(viewModel.previewAlternatives, id: \.id) { alternative in
                Button {
                    viewModel.selectAlternative(alternative.id)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(alternative.title)
                                .font(.subheadline.weight(.semibold))
                            Text(alternative.normalizedPackage.summaryLine)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if alternative.id == appModel.preview.selectedAlternativeID {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.blue)
                        }
                    }
                    .padding(12)
                    .background(alternative.id == appModel.preview.selectedAlternativeID ? Color.blue.opacity(0.12) : Color.clear, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
                .buttonStyle(.plain)
            }
            Button(action: viewModel.startSelectedRoute) {
                Text("Start")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
    }

    private func activeGuidanceCard(_ active: NormalizedRoutePackage) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(active.summary.destinationLabel ?? "Guidance active")
                .font(.headline)
            Text(active.summaryLine)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Button(role: .destructive, action: viewModel.stopGuidance) {
                Text("Stop")
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
    }

    private func centerCamera(on coordinate: CLLocationCoordinate2D) {
        cameraPosition = .region(MKCoordinateRegion(center: coordinate, span: MKCoordinateSpan(latitudeDelta: 0.025, longitudeDelta: 0.025)))
    }
}
