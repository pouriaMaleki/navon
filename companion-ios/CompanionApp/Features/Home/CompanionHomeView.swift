import SwiftUI
import MapKit

struct CompanionHomeView: View {
    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: HomeViewModel
    let onOpenSettings: () -> Void

    @State private var cameraPosition: MapCameraPosition = .region(
        MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: 60.1699, longitude: 24.9384),
            span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
        )
    )
    @FocusState private var isSearchFieldFocused: Bool

    var body: some View {
        MapReader { proxy in
            mapSurface(proxy: proxy)
                .overlay(alignment: .top) {
                    topOverlay
                }
                .overlay(alignment: .bottom) {
                    bottomOverlay
                }
                .ignoresSafeArea(edges: .bottom)
                .onAppear { refreshCameraForCurrentMode() }
                .onChange(of: appModel.preview.selectedAlternativeID) { _, _ in refreshCameraForCurrentMode() }
                .onChange(of: viewModel.activeRouteIdentifier) { _, _ in refreshCameraForCurrentMode() }
                .onChange(of: viewModel.homeMode) { _, newValue in
                    refreshCameraForCurrentMode()
                    if newValue != .planning {
                        isSearchFieldFocused = false
                    }
                }
                .onChange(of: viewModel.compassMode) { _, _ in refreshCameraForCurrentMode() }
                .onChange(of: viewModel.isSearchOpen) { _, isOpen in
                    if !isOpen {
                        isSearchFieldFocused = false
                    }
                }
        }
    }

    @ViewBuilder
    private func mapSurface(proxy: MapProxy) -> some View {
        Map(position: $cameraPosition) {
            UserAnnotation()

            if let start = viewModel.originCoordinate {
                Marker("Start", coordinate: CLLocationCoordinate2D(latitude: start.latitude, longitude: start.longitude))
                    .tint(.green)
            }

            if let destination = viewModel.destinationCoordinate {
                Marker("Destination", coordinate: CLLocationCoordinate2D(latitude: destination.latitude, longitude: destination.longitude))
                    .tint(.red)
            }

            if viewModel.homeMode == .planning {
                ForEach(viewModel.previewAlternatives, id: \.id) { alternative in
                    let isSelected = alternative.id == appModel.preview.selectedAlternativeID
                    MapPolyline(coordinates: alternative.normalizedPackage.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                        .stroke(isSelected ? .blue : .teal.opacity(0.45), lineWidth: isSelected ? 6 : 4)
                }
            } else if let active = viewModel.guidanceRoute {
                MapPolyline(coordinates: active.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                    .stroke(viewModel.homeMode == .deviceOverview ? .blue : .green, lineWidth: 7)
            }
        }
        .mapControls {
            MapCompass()
            MapUserLocationButton()
        }
        .simultaneousGesture(
            LongPressGesture(minimumDuration: 0.6)
                .sequenced(before: DragGesture(minimumDistance: 0, coordinateSpace: .local))
                .onEnded { value in
                    guard viewModel.homeMode == .planning else { return }
                    switch value {
                    case .second(true, let drag?):
                        if let coordinate = proxy.convert(drag.location, from: .local) {
                            viewModel.setDestinationFromMap(CoordinatePoint(latitude: coordinate.latitude, longitude: coordinate.longitude))
                        }
                    default:
                        break
                    }
                }
        )
    }

    private var topOverlay: some View {
        VStack(spacing: 10) {
            switch viewModel.homeMode {
            case .planning:
                planningTopOverlay
            case .phoneGuidance:
                phoneGuidanceTopOverlay
            case .sendingToDevice, .deviceOverview:
                deviceOverviewTopOverlay
            }
        }
        .padding(.horizontal, 16)
        .padding(.top, 8)
    }

    @ViewBuilder
    private var bottomOverlay: some View {
        switch viewModel.homeMode {
        case .planning:
            if !viewModel.previewAlternatives.isEmpty {
                routeSuggestionsCard
            }
        case .phoneGuidance:
            if let active = viewModel.guidanceRoute {
                activeGuidanceCard(active)
            }
        case .sendingToDevice, .deviceOverview:
            if let active = viewModel.guidanceRoute {
                deviceOverviewCard(active)
            }
        }
    }

    private var planningTopOverlay: some View {
        VStack(spacing: 8) {
            topBar
            if viewModel.shouldShowSearchPanel {
                searchPanel
            }
        }
    }

    private var phoneGuidanceTopOverlay: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(viewModel.activeNavigationTitle)
                    .font(.headline)
                    .lineLimit(1)
                Text(viewModel.activeNavigationSubtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer()
            Image(systemName: viewModel.compassSymbolName)
                .font(.system(size: 20, weight: .semibold))
                .foregroundStyle(.primary)
                .frame(width: 48, height: 48)
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
                .onTapGesture(count: 2) { viewModel.handleCompassDoubleTap() }
                .onTapGesture { viewModel.handleCompassTap() }
                .accessibilityLabel("North indicator")
        }
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var deviceOverviewTopOverlay: some View {
        HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(viewModel.activeNavigationTitle)
                    .font(.headline)
                    .lineLimit(1)
                Text(viewModel.activeNavigationSubtitle)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer()
            Button(action: onOpenSettings) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .frame(width: 48, height: 48)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 16, style: .continuous))
            }
            .accessibilityLabel("Settings")
        }
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var topBar: some View {
        HStack(spacing: 10) {
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
                .submitLabel(.done)
                .focused($isSearchFieldFocused)
                .onTapGesture {
                    viewModel.openSearch()
                    isSearchFieldFocused = true
                }

                if !viewModel.query.isEmpty || !viewModel.previewAlternatives.isEmpty || viewModel.isShowingActiveNavigation {
                    Button {
                        isSearchFieldFocused = false
                        viewModel.clearPreview()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityLabel("Clear destination")
                }
            }
            .padding(14)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))

            Button(action: onOpenSettings) {
                Image(systemName: "gearshape.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .frame(width: 50, height: 50)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
            }
            .accessibilityLabel("Settings")
        }
    }

    private var sourceModePicker: some View {
        HStack(spacing: 8) {
            ForEach(appModel.sourceModeOptions) { mode in
                Button {
                    viewModel.setSourceMode(mode)
                } label: {
                    Text(mode.displayName)
                        .font(.subheadline.weight(.semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 8)
                        .frame(maxWidth: .infinity)
                        .background(
                            viewModel.sourceMode == mode ? Color.blue.opacity(0.16) : Color.clear,
                            in: RoundedRectangle(cornerRadius: 14, style: .continuous)
                        )
                }
                .buttonStyle(.plain)
            }
        }
        .padding(8)
        .background(.thinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var searchPanel: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if viewModel.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ForEach(viewModel.recentItems) { item in
                        Button {
                            isSearchFieldFocused = false
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
                            isSearchFieldFocused = false
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
            HStack {
                Text(viewModel.routeSuggestionsTitle)
                    .font(.headline)
                Spacer()
                Button("Close") {
                    isSearchFieldFocused = false
                    viewModel.clearPreview()
                }
                .font(.subheadline.weight(.semibold))
            }

            if let notice = appModel.preview.planningNotice {
                Text(notice)
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }

            if viewModel.shouldShowSourceControl {
                sourceModePicker
            }

            ForEach(viewModel.previewAlternatives, id: \.id) { alternative in
                Button {
                    viewModel.selectAlternative(alternative.id)
                } label: {
                    HStack {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(alternative.title)
                                .font(.subheadline.weight(.semibold))
                            Text(alternative.subtitle)
                                .font(.caption)
                                .foregroundStyle(.secondary)
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
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .padding(12)
                    .background(alternative.id == appModel.preview.selectedAlternativeID ? Color.blue.opacity(0.12) : Color.clear, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
                }
                .buttonStyle(.plain)
            }

            Button {
                isSearchFieldFocused = false
                Task { await viewModel.startSelectedRoute() }
            } label: {
                Group {
                    if viewModel.homeMode == .sendingToDevice {
                        HStack(spacing: 8) {
                            ProgressView()
                                .tint(.white)
                            Text(viewModel.startButtonTitle)
                        }
                        .frame(maxWidth: .infinity)
                    } else {
                        Text(viewModel.startButtonTitle)
                            .frame(maxWidth: .infinity)
                    }
                }
            }
            .buttonStyle(.borderedProminent)
            .disabled(viewModel.homeMode == .sendingToDevice)
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
            Text(viewModel.nextInstructionLine ?? active.summaryLine)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            Button(role: .destructive, action: viewModel.stopActiveNavigation) {
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

    private func deviceOverviewCard(_ active: NormalizedRoutePackage) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(active.summary.destinationLabel ?? "Route active on device")
                .font(.headline)
            Text(appModel.bleService.sessionState.lastSyncResult)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let transfer = appModel.bleService.sessionState.transferProgress {
                Text("Sending \(transfer.percentComplete)%")
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Button(role: .destructive, action: viewModel.stopActiveNavigation) {
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

    private func refreshCameraForCurrentMode() {
        switch viewModel.homeMode {
        case .planning, .deviceOverview, .sendingToDevice:
            let coordinates = viewModel.displayedRouteCoordinates
            guard !coordinates.isEmpty else { return }
            fitCamera(to: coordinates)
        case .phoneGuidance:
            guard let route = viewModel.guidanceRoute else { return }
            switch viewModel.compassMode {
            case .autoFollow:
                orientCameraForTravel(on: route)
            case .northPreview, .northLocked:
                fitCamera(to: route.geometry)
            }
        }
    }

    private func orientCameraForTravel(on route: NormalizedRoutePackage) {
        guard route.geometry.count >= 2 else {
            fitCamera(to: route.geometry)
            return
        }
        let anchor = route.geometry[0]
        let next = route.geometry[1]
        let heading = bearingDegrees(from: anchor, to: next)
        let center = CLLocationCoordinate2D(latitude: anchor.latitude, longitude: anchor.longitude)
        cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: 1200, heading: heading, pitch: 0))
    }

    private func fitCamera(to coordinates: [CoordinatePoint]) {
        guard let first = coordinates.first else { return }
        var minLat = first.latitude
        var maxLat = first.latitude
        var minLon = first.longitude
        var maxLon = first.longitude

        for point in coordinates.dropFirst() {
            minLat = min(minLat, point.latitude)
            maxLat = max(maxLat, point.latitude)
            minLon = min(minLon, point.longitude)
            maxLon = max(maxLon, point.longitude)
        }

        let latDelta = max((maxLat - minLat) * 1.5, 0.01)
        let lonDelta = max((maxLon - minLon) * 1.5, 0.01)
        let center = CLLocationCoordinate2D(latitude: (minLat + maxLat) / 2.0, longitude: (minLon + maxLon) / 2.0)
        cameraPosition = .region(MKCoordinateRegion(center: center, span: MKCoordinateSpan(latitudeDelta: latDelta, longitudeDelta: lonDelta)))
    }

    private func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> CLLocationDirection {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return atan2(lonMeters, latMeters) * 180.0 / .pi
    }
}
