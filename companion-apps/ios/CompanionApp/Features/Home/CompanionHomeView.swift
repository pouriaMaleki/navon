import SwiftUI
import MapKit

struct CompanionHomeView: View {
    @EnvironmentObject private var appModel: AppModel
    @EnvironmentObject private var shareImportService: ShareImportService
    @EnvironmentObject private var searchController: SearchController
    @ObservedObject var viewModel: HomeViewModel
    let onOpenSettings: () -> Void

    @State private var cameraPosition: MapCameraPosition = .region(
        MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: 60.1699, longitude: 24.9384),
            span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
        )
    )
    @State private var planningCameraReference: PlanningCameraReference?
    /// Used to distinguish programmatic camera moves from user gestures —
    /// changes within `programmaticCameraQuietWindow` of a set are treated as
    /// MapKit animating to our target, not a user interaction.
    ///
    /// Stored on a class wrapper instead of `@State Date` so that writes from
    /// the high-frequency camera-follow path don't invalidate the view body.
    /// Body never reads this — only `.onMapCameraChange` does — so a write-
    /// triggered `@State` invalidation would be pure waste.
    @State private var cameraTimestamps = CameraTimestamps()
    private let programmaticCameraQuietWindow: TimeInterval = 0.6
    @FocusState private var isSearchFieldFocused: Bool

    var body: some View {
        // Compute once per render so we don't pay 3× persistence + recompute
        // (the value is read by the planning overlay and both side rails).
        let isSearchPanelVisible = viewModel.shouldShowSearchPanel
        return MapReader { proxy in
            mapSurface(proxy: proxy)
                .overlay(alignment: .top) {
                    topOverlay(isSearchPanelVisible: isSearchPanelVisible)
                }
                .overlay(alignment: .bottom) {
                    bottomOverlay
                }
                .overlay(alignment: .bottomLeading) {
                    speedBadge
                }
                .overlay(alignment: .topTrailing) {
                    rightSideRailTop(isSearchPanelVisible: isSearchPanelVisible)
                }
                .overlay(alignment: .topLeading) {
                    leftSideRailTop(isSearchPanelVisible: isSearchPanelVisible)
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
                .onChange(of: searchController.isSearchOpen) { _, isOpen in
                    if !isOpen {
                        isSearchFieldFocused = false
                    }
                }
                .onMapCameraChange(frequency: .continuous) { context in
                    let sinceProgrammatic = Date().timeIntervalSince(cameraTimestamps.lastProgrammaticCameraSetAt)
                    let isLikelyUser = sinceProgrammatic > programmaticCameraQuietWindow
                    if isLikelyUser {
                        viewModel.noteUserMapInteraction()
                    }
                }
                .onChange(of: viewModel.mapRecenterRequestID) { _, _ in
                    refreshCameraForCurrentMode()
                }
                .onChange(of: viewModel.mapFollowRiderTick) { _, _ in
                    // Single camera-refresh signal per GPS fix. `mapFollowRiderTick`
                    // bumps on every fix; `progressDistanceM` only updates when the
                    // rider has advanced along the route, but it always co-fires
                    // with this tick (see `.onChange(of: bestLocation)` below —
                    // `ingestRiderLocationFix` updates progress, then
                    // `notifyRiderLocationUpdated` bumps the tick). Watching this
                    // tick alone covers both, and avoids two camera refreshes per
                    // fix (= two `cameraPosition` @State writes = two body re-renders).
                    // Suppress while the user is manually panning/zooming.
                    if !viewModel.isUserInteractingWithMap {
                        refreshCameraForCurrentMode()
                    }
                }
                .onChange(of: appModel.locationService.bestLocation) { _, newValue in
                    viewModel.ingestRiderLocationFix(newValue, timestampMs: Int64(Date().timeIntervalSince1970 * 1000))
                    viewModel.notifyRiderLocationUpdated()
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
                let exploreSelected = viewModel.selectedAlternativeIDForDisplay
                // Dim the active route when the user taps an alternative during exploration.
                let activeRouteColor: Color = (viewModel.isExploringAlternativesFromGuidance && exploreSelected != nil)
                    ? .teal.opacity(0.45)
                    : (viewModel.homeMode == .deviceOverview ? .blue : .green)
                MapPolyline(coordinates: active.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                    .stroke(activeRouteColor, lineWidth: 7)
                ForEach(viewModel.guidanceAlternatives, id: \.id) { alt in
                    let isSelected = alt.id == exploreSelected
                    MapPolyline(coordinates: alt.normalizedPackage.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                        .stroke(isSelected ? Color.blue : .teal.opacity(0.45), lineWidth: isSelected ? 6 : 4)
                }
            }
        }
        .mapControlVisibility(.hidden)
        .overlay(alignment: .topTrailing) {
            planningMapAccessoryControls
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
        .simultaneousGesture(
            TapGesture().onEnded {
                guard viewModel.homeMode == .planning, searchController.isSearchOpen else { return }
                isSearchFieldFocused = false
                searchController.closeSearch()
            }
        )
    }

    @ViewBuilder
    private func topOverlay(isSearchPanelVisible: Bool) -> some View {
        VStack(spacing: 10) {
            switch viewModel.homeMode {
            case .planning:
                planningTopOverlay(isSearchPanelVisible: isSearchPanelVisible)
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
    private var speedBadge: some View {
        let moving = viewModel.travelHeadingDegrees != nil
        let inGuidance = viewModel.homeMode == .phoneGuidance
        let suggestionsVisible = viewModel.isExploringAlternativesFromGuidance ||
            (viewModel.homeMode == .planning && (!appModel.preview.alternatives.isEmpty || appModel.preview.planningNotice != nil))
        if (moving || inGuidance) && !suggestionsVisible {
            Text(formatSpeed(
                appModel.locationService.currentSpeedMps,
                unit: appModel.settings.speedUnit
            ))
            .font(.system(size: 14, weight: .bold))
            .padding(.horizontal, 12)
            .padding(.vertical, 8)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 14, style: .continuous))
            .padding(.leading, 16)
            .padding(.bottom, 32)
            .accessibilityLabel(T.string("home.a11y.currentSpeed"))
        }
    }

    private static let railIconSize: CGFloat = 50
    private static let railIconCorner: CGFloat = 18
    private static let railIconSpacing: CGFloat = 8

    /// Shared Y offset for both rails, sized to clear the tallest routing card (3-line + off-route pill).
    private static let railTopPadding: CGFloat = 130

    @ViewBuilder
    private func rightSideRailTop(isSearchPanelVisible: Bool) -> some View {
        if isSearchPanelVisible {
            EmptyView()
        } else {
            VStack(spacing: Self.railIconSpacing) {
                ForEach(0..<viewModel.topRightIconStack.count, id: \.self) { idx in
                    topRailIcon(viewModel.topRightIconStack[idx])
                }
            }
            .padding(.top, Self.railTopPadding)
            .padding(.trailing, 12)
        }
    }

    @ViewBuilder
    private func leftSideRailTop(isSearchPanelVisible: Bool) -> some View {
        if isSearchPanelVisible {
            EmptyView()
        } else {
            VStack(spacing: Self.railIconSpacing) {
                ForEach(0..<viewModel.topLeftIconStack.count, id: \.self) { idx in
                    leftRailIcon(viewModel.topLeftIconStack[idx])
                }
            }
            .padding(.top, Self.railTopPadding)
            .padding(.leading, 16)
        }
    }

    @ViewBuilder
    private func topRailIcon(_ item: HomeViewModel.TopRightIcon) -> some View {
        switch item {
        case .settings:
            Button(action: onOpenSettings) {
                railGlyph("gearshape.fill")
            }
            .buttonStyle(.plain)
            .accessibilityLabel(T.string("home.a11y.settings"))
        case .compass:
            // Tap = recenter, double-tap = lock north-up.
            railGlyph(viewModel.compassSymbolName)
                .onTapGesture(count: 2) { viewModel.handleCompassDoubleTap() }
                .onTapGesture { viewModel.handleCompassTap() }
                .accessibilityLabel(T.string("home.a11y.recenterMap"))
        case .deviceChip:
            if let chipState = viewModel.deviceChipState {
                DeviceStatusChip(state: chipState) {
                    viewModel.handleDeviceChipTap()
                }
            }
        }
    }

    @ViewBuilder
    private func leftRailIcon(_ item: HomeViewModel.TopLeftIcon) -> some View {
        switch item {
        case .alternateRoutes:
            Button {
                viewModel.exploreAlternateRoutes()
            } label: {
                railGlyph("arrow.triangle.branch")
            }
            .buttonStyle(.plain)
            .accessibilityLabel(T.string("home.a11y.findAlternates"))
        case .zoomIn:
            zoomButton(symbol: "plus", label: "Zoom in") { applyZoom(direction: .zoomIn) }
        case .zoomOut:
            zoomButton(symbol: "minus", label: "Zoom out") { applyZoom(direction: .zoomOut) }
        }
    }

    private func railGlyph(_ systemName: String) -> some View {
        Image(systemName: systemName)
            .font(.system(size: 18, weight: .semibold))
            .foregroundStyle(.primary)
            .frame(width: Self.railIconSize, height: Self.railIconSize)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: Self.railIconCorner, style: .continuous))
    }

    private func zoomButton(symbol: String, label: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 18, weight: .semibold))
                .foregroundStyle(.primary)
                .frame(width: 50, height: 50)
                .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
        .buttonStyle(.plain)
        .accessibilityLabel(label)
    }

    private enum ZoomDirection { case zoomIn, zoomOut }

    private func applyZoom(direction: ZoomDirection) {
        switch viewModel.homeMode {
        case .phoneGuidance:
            viewModel.bumpRidingZoom(direction: direction == .zoomIn ? .zoomIn : .zoomOut)
            let next = viewModel.ridingCameraDistanceM
            if let route = viewModel.guidanceRoute {
                let rider = appModel.locationService.bestLocation
                let heading = viewModel.cameraHeadingDegrees(rider: rider)
                    ?? bearingDegrees(from: route.geometry.first ?? rider,
                                      to: route.geometry.dropFirst().first ?? rider)
                // Anchor offset must scale with camera distance to keep rider in the bottom quarter.
                let centerPoint = CameraMath.cameraCenterCoordinate(rider: rider, headingDegrees: heading, cameraDistanceM: next)
                let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
                withAnimation(.easeInOut(duration: 0.25)) {
                    cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: next, heading: heading, pitch: 0))
                }
                cameraTimestamps.lastProgrammaticCameraSetAt = Date()
            } else {
                refreshCameraForCurrentMode()
            }
        case .planning, .deviceOverview, .sendingToDevice:
            // MapCameraPosition is opaque — use planningCameraReference as the zoom source
            // (kept in sync by onMapCameraChange), falling back to rider location.
            let factor: Double = direction == .zoomIn
                ? (1.0 / CameraMath.ridingZoomStepFactor)
                : CameraMath.ridingZoomStepFactor
            let rider = appModel.locationService.bestLocation
            let baseSpan = planningCameraReference?.span
                ?? MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
            let baseCenter = planningCameraReference?.center
                ?? CLLocationCoordinate2D(latitude: rider.latitude, longitude: rider.longitude)
            let scaledLat = min(20.0, max(0.0008, baseSpan.latitudeDelta * factor))
            let scaledLon = min(20.0, max(0.0008, baseSpan.longitudeDelta * factor))
            let scaledSpan = MKCoordinateSpan(latitudeDelta: scaledLat, longitudeDelta: scaledLon)
            setCamera(
                region: MKCoordinateRegion(center: baseCenter, span: scaledSpan),
                heading: 0,
                recordPlanningReference: false
            )
        }
    }

    private func formatSpeed(_ mps: Double?, unit: SpeedUnit) -> String {
        let factor = unit == .mph ? 2.2369363 : 3.6
        guard let mps, mps.isFinite else {
            return "— \(unit.label)"
        }
        return "\(Int((mps * factor).rounded())) \(unit.label)"
    }

    @ViewBuilder
    private var bottomOverlay: some View {
        switch viewModel.homeMode {
        case .planning:
            if let arrival = viewModel.arrivalNotice {
                arrivalCard(arrival)
            } else if viewModel.planningStatus != nil || shareImportService.importActivityStatus != nil {
                planningProgressCard
            } else if !viewModel.previewAlternatives.isEmpty || appModel.preview.planningNotice != nil {
                routeSuggestionsCard
            }
        case .phoneGuidance:
            if viewModel.isExploringAlternativesFromGuidance {
                routeSuggestionsCard
            } else if let active = viewModel.guidanceRoute {
                activeGuidanceCard(active)
            }
        case .sendingToDevice, .deviceOverview:
            if let active = viewModel.guidanceRoute {
                deviceOverviewCard(active)
            }
        }
    }

    private func arrivalCard(_ message: String) -> some View {
        ArrivalCardView(message: message, onDismiss: { viewModel.dismissArrivalNotice() })
    }

    private func planningTopOverlay(isSearchPanelVisible: Bool) -> some View {
        VStack(spacing: 8) {
            topBar
            if isSearchPanelVisible {
                searchPanel
            }
        }
    }

    private var phoneGuidanceTopOverlay: some View {
        if viewModel.isExploringAlternativesFromGuidance {
            return AnyView(explorationDestinationBar)
        }
        let layout = viewModel.routingTopLayout
        return AnyView(VStack(spacing: 6) {
            if viewModel.isWaitingToReroute(now: Date().timeIntervalSince1970 * 1_000) {
                rerouteWaitingPill
            } else if let offLabel = layout?.offRouteLabel {
                Text(offLabel)
                    .font(.subheadline.weight(.bold))
                    .foregroundStyle(.black)
                    .frame(maxWidth: .infinity)
                    .padding(.vertical, 6)
                    .padding(.horizontal, 12)
                    .background(viewModel.rerouteRequested ? Color.cyan : Color.yellow,
                                in: RoundedRectangle(cornerRadius: 10, style: .continuous))
            }
            phoneGuidanceTopCard(
                headline: layout?.headline ?? viewModel.activeNavigationTitle,
                distanceLine: layout?.distanceToDestinationLine ?? "",
                minutesLine: layout?.minutesRemainingLine ?? ""
            )
        })
    }

    private var rerouteWaitingPill: some View {
        RerouteWaitingPillView(
            delayedUntilMs: viewModel.reroutingDelayedUntilMs,
            onRerouteNow: { viewModel.requestManualReroute() }
        )
    }

    private var explorationDestinationBar: some View {
        ExplorationDestinationBarView(
            query: searchController.query,
            navigationTitle: viewModel.activeNavigationTitle
        )
    }

    private func phoneGuidanceTopCard(
        headline: String,
        distanceLine: String,
        minutesLine: String
    ) -> some View {
        PhoneGuidanceTopCardView(headline: headline, distanceLine: distanceLine, minutesLine: minutesLine)
    }

    private var deviceOverviewTopOverlay: some View {
        DeviceOverviewCardView(
            title: viewModel.activeNavigationTitle,
            subtitle: viewModel.activeNavigationSubtitle
        )
    }

    private var topBar: some View {
        HStack(spacing: 10) {
            HStack(spacing: 12) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(T.string("home.whereTo"), text: Binding(
                    get: { searchController.query },
                    set: { newValue in
                        // Guard against SwiftUI echoing set(currentValue) after a
                        // programmatic query change (e.g. picking a recent) — that
                        // would otherwise re-open the panel and trigger a fresh
                        // search for the address the user just picked.
                        guard newValue != searchController.query else { return }
                        searchController.updateQuery(newValue)
                    }
                ))
                .textInputAutocapitalization(.words)
                .autocorrectionDisabled()
                .submitLabel(.done)
                .accessibilityIdentifier("whereToInput")
                .focused($isSearchFieldFocused)
                .onChange(of: isSearchFieldFocused) { _, focused in
                    if focused { searchController.openSearch() }
                }

                if !searchController.query.isEmpty || !viewModel.previewAlternatives.isEmpty || viewModel.isShowingActiveNavigation {
                    Button {
                        isSearchFieldFocused = false
                        viewModel.clearPreview()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                    .accessibilityLabel(T.string("home.a11y.clearDestination"))
                }
            }
            .padding(14)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
        }
    }

    private var sourceModePicker: some View {
        SourceModePickerView(
            modes: HslAvailabilityService.sourceModeOptions(request: appModel.routeRequest),
            currentMode: viewModel.sourceMode,
            onSelect: { viewModel.setSourceMode($0) }
        )
    }

    private var searchPanel: some View {
        SearchPanelView(viewModel: viewModel, isSearchFieldFocused: $isSearchFieldFocused)
    }

    @ViewBuilder
    private var planningMapAccessoryControls: some View {
        if viewModel.homeMode == .planning {
            if isWaitingForFirstLocationFix {
                ProgressView()
                    .progressViewStyle(.circular)
                    .controlSize(.small)
                    .frame(width: 50, height: 50)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .accessibilityLabel(T.string("home.a11y.locating"))
                    .padding(.trailing, 76)
                    .padding(.top, 72)
            } else if appModel.locationService.lastError == .denied {
                Image(systemName: "location.slash.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .frame(width: 50, height: 50)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .accessibilityLabel(T.string("home.a11y.locationBlocked"))
                    .padding(.trailing, 76)
                    .padding(.top, 72)
            }
        }
    }

    private var isWaitingForFirstLocationFix: Bool {
        appModel.locationService.isLocating
            && appModel.locationService.currentLocation == nil
            && appModel.locationService.lastKnownLocation == nil
    }

    private var planningProgressCard: some View {
        PlanningProgressCardView(
            status: viewModel.planningStatus ?? shareImportService.importActivityStatus ?? "Planning route…"
        )
    }

    private var routeSuggestionsCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack {
                Text(viewModel.routeSuggestionsTitle)
                    .font(.headline)
                Spacer()
                Button(T.string("common.close")) {
                    isSearchFieldFocused = false
                    if viewModel.isExploringAlternativesFromGuidance {
                        viewModel.cancelAlternativesExploration()
                    } else {
                        viewModel.clearPreview()
                    }
                }
                .font(.subheadline.weight(.semibold))
            }

            if viewModel.isExploringAlternativesFromGuidance {
                let continueSelected = viewModel.selectedAlternativeIDForDisplay == nil
                Button {
                    viewModel.deselectForExploration()
                } label: {
                    HStack(spacing: 10) {
                        Text(T.string("home.continueOnCurrentRoute"))
                            .font(.subheadline.weight(.semibold))
                        Spacer()
                        if continueSelected {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.blue)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(continueSelected ? Color.blue.opacity(0.12) : Color.clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(.plain)
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
                    HStack(spacing: 10) {
                        VStack(alignment: .leading, spacing: 1) {
                            Text(alternative.title)
                                .font(.subheadline.weight(.semibold))
                            if !alternative.subtitle.isEmpty {
                                Text(alternative.subtitle)
                                    .font(.caption)
                                    .foregroundStyle(.secondary)
                            }
                            Text(alternative.normalizedPackage.summaryLine)
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        if alternative.id == viewModel.selectedAlternativeIDForDisplay {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.blue)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .contentShape(Rectangle())
                    .padding(.horizontal, 10)
                    .padding(.vertical, 6)
                    .background(alternative.id == viewModel.selectedAlternativeIDForDisplay ? Color.blue.opacity(0.12) : Color.clear, in: RoundedRectangle(cornerRadius: 12, style: .continuous))
                }
                .buttonStyle(.plain)
            }

            Button {
                isSearchFieldFocused = false
                if viewModel.isExploringAlternativesFromGuidance && viewModel.selectedAlternativeIDForDisplay == nil {
                    viewModel.cancelAlternativesExploration()
                } else {
                    Task { await viewModel.startSelectedRoute() }
                }
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
            .disabled(
                viewModel.homeMode == .sendingToDevice ||
                (viewModel.previewAlternatives.isEmpty && !viewModel.isExploringAlternativesFromGuidance)
            )
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
    }

    private func activeGuidanceCard(_ active: NormalizedRoutePackage) -> some View {
        ActiveGuidanceCardView(onStop: { viewModel.stopActiveNavigation() })
    }

    private func deviceOverviewCard(_ active: NormalizedRoutePackage) -> some View {
        VStack(alignment: .leading, spacing: 12) {
            Text(active.summary.destinationLabel ?? T.string("home.routeActiveOnDevice"))
                .font(.headline)
            Text(appModel.bleService.sessionState.lastSyncResult)
                .font(.subheadline)
                .foregroundStyle(.secondary)
            if let transfer = appModel.bleService.sessionState.transferProgress {
                Text(T.string(
                    "home.sendingPercent",
                    ["percent": .number(Double(transfer.percentComplete))]
                ))
                    .font(.caption)
                    .foregroundStyle(.secondary)
            }
            Button(role: .destructive, action: { viewModel.stopActiveNavigation() }) {
                Text(T.string("home.stop"))
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
        case .planning:
            resetPlanningCamera()
        case .deviceOverview, .sendingToDevice:
            let coordinates = viewModel.displayedRouteCoordinates
            guard !coordinates.isEmpty else { return }
            fitCamera(to: coordinates)
        case .phoneGuidance:
            guard let route = viewModel.guidanceRoute else { return }
            switch viewModel.compassMode {
            case .autoFollow:
                orientCameraForTravel(on: route)
            case .northPreview, .northLocked:
                let overview = viewModel.routeOverviewGeometry
                fitCamera(to: overview.count >= 2 ? overview : route.geometry)
            }
        }
    }

    private func resetPlanningCamera() {
        let coordinates = viewModel.displayedRouteCoordinates
        if let trailHeading = viewModel.travelHeadingDegrees {
            let rider = appModel.locationService.bestLocation
            let centerPoint = CameraMath.cameraCenterCoordinate(
                rider: rider, headingDegrees: trailHeading, cameraDistanceM: viewModel.ridingCameraDistanceM
            )
            let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
            // Animate camera moves to avoid hard-snapping on each GPS fix.
            withAnimation(.easeInOut(duration: 0.22)) {
                cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: viewModel.ridingCameraDistanceM, heading: trailHeading, pitch: 0))
            }
            cameraTimestamps.lastProgrammaticCameraSetAt = Date()
            return
        }
        if !coordinates.isEmpty {
            fitCamera(to: coordinates, recordPlanningReference: true)
        } else {
            let rider = appModel.locationService.bestLocation
            setCamera(
                region: MKCoordinateRegion(
                    center: CLLocationCoordinate2D(latitude: rider.latitude, longitude: rider.longitude),
                    span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
                ),
                heading: 0,
                recordPlanningReference: true
            )
        }
    }

    private func orientCameraForTravel(on route: NormalizedRoutePackage) {
        guard route.geometry.count >= 2 else {
            fitCamera(to: route.geometry)
            return
        }
        let rider = appModel.locationService.bestLocation
        let heading = viewModel.cameraHeadingDegrees(rider: rider)
            ?? bearingDegrees(from: route.geometry[0], to: route.geometry[1])
        // Shift camera center ahead of rider so they render in the bottom quarter.
        // Anchor offset must scale with camera distance — deep zoom-in pushes rider off the bottom edge otherwise.
        let centerPoint = CameraMath.cameraCenterCoordinate(
            rider: rider, headingDegrees: heading, cameraDistanceM: viewModel.ridingCameraDistanceM
        )
        let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
        // Camera distance from viewModel (not hardcoded) — +/- buttons and autoFollow share the same persisted value.
        withAnimation(.easeInOut(duration: 0.22)) {
            cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: viewModel.ridingCameraDistanceM, heading: heading, pitch: 0))
        }
        cameraTimestamps.lastProgrammaticCameraSetAt = Date()
    }

    private func fitCamera(to coordinates: [CoordinatePoint], recordPlanningReference: Bool = false) {
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

        let region = CameraMath.fittedRouteRegion(
            minLat: minLat, maxLat: maxLat,
            minLon: minLon, maxLon: maxLon
        )
        setCamera(region: region, heading: 0, recordPlanningReference: recordPlanningReference)
    }

    private func setCamera(region: MKCoordinateRegion, heading: CLLocationDirection, recordPlanningReference: Bool) {
        withAnimation(.easeInOut(duration: 0.45)) {
            cameraPosition = .region(region)
        }
        cameraTimestamps.lastProgrammaticCameraSetAt = Date()
        if recordPlanningReference {
            planningCameraReference = PlanningCameraReference(center: region.center, span: region.span, heading: heading)
        }
    }

    private struct PlanningCameraReference {
        let center: CLLocationCoordinate2D
        let span: MKCoordinateSpan
        let heading: CLLocationDirection
    }

    /// PERF: holder for view-local state that body must NOT depend on.
    ///
    /// `@State` invalidates the view on every write, regardless of whether
    /// body reads the value. We mutate `lastProgrammaticCameraSetAt` on every
    /// programmatic camera nudge (setCamera, applyZoom, follow-rider
    /// orientation) — that's many writes per second during navigation.
    /// Body never reads this field; only `.onMapCameraChange` does. So we
    /// store it on a class reference held via `@State`: assigning fields
    /// through the reference bypasses the property wrapper's setter, so no
    /// invalidation fires.
    ///
    /// DO NOT "simplify" this back to `@State private var lastProgrammaticCameraSetAt: Date`
    /// — that reintroduces ~10 redundant body re-renders per navigation
    /// second (verified via PerfLog instrumentation, removed afterwards).
    private final class CameraTimestamps {
        var lastProgrammaticCameraSetAt: Date = .distantPast
    }
}
