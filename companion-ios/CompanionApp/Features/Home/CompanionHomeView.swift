import SwiftUI
import MapKit

struct CompanionHomeView: View {
    private struct PlanningCameraReference {
        let center: CLLocationCoordinate2D
        let span: MKCoordinateSpan
        let heading: CLLocationDirection
    }

    @EnvironmentObject private var appModel: AppModel
    @ObservedObject var viewModel: HomeViewModel
    let onOpenSettings: () -> Void

    @State private var cameraPosition: MapCameraPosition = .region(
        MKCoordinateRegion(
            center: CLLocationCoordinate2D(latitude: 60.1699, longitude: 24.9384),
            span: MKCoordinateSpan(latitudeDelta: 0.03, longitudeDelta: 0.03)
        )
    )
    @State private var planningCameraReference: PlanningCameraReference?
    @State private var planningCameraNeedsReset = false
    /// Timestamp of the last programmatic `cameraPosition = ...` set. MapKit's
    /// `onMapCameraChange(.continuous)` fires for both user gestures and our
    /// own animations, so we use a short quiet-window to distinguish: camera
    /// changes within `programmaticCameraQuietWindow` of a programmatic set
    /// are assumed to be MapKit animating to our target, not a user gesture.
    @State private var lastProgrammaticCameraSetAt: Date = .distantPast
    private let programmaticCameraQuietWindow: TimeInterval = 0.6
    @FocusState private var isSearchFieldFocused: Bool

    /// Camera distance (m) used for the riding-mode follow-rider camera.
    /// Authoritative value lives on the viewModel so both `applyZoom` and
    /// `orientCameraForTravel` read from the same source — that's what
    /// keeps the rider's preferred zoom alive across compass cycles +
    /// GPS-fix-driven camera refreshes.
    private var ridingCameraDistanceM: Double { viewModel.ridingCameraDistanceM }

    var body: some View {
        MapReader { proxy in
            mapSurface(proxy: proxy)
                .overlay(alignment: .top) {
                    topOverlay
                }
                .overlay(alignment: .bottom) {
                    bottomOverlay
                }
                .overlay(alignment: .bottomLeading) {
                    speedBadge
                }
                .overlay(alignment: .topTrailing) {
                    rightSideRailTop
                }
                .overlay(alignment: .topLeading) {
                    leftSideRailTop
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
                .onMapCameraChange(frequency: .continuous) { context in
                    updatePlanningCameraResetState(for: context.region, heading: context.camera.heading)
                    // Spec line 104: user pans/pinches/rotates during routing
                    // should schedule an auto-recenter. But `onMapCameraChange`
                    // fires on BOTH user gestures and our own programmatic
                    // animations, so treat anything inside the quiet-window
                    // after a programmatic set as "not a user gesture".
                    let sinceProgrammatic = Date().timeIntervalSince(lastProgrammaticCameraSetAt)
                    let isLikelyUser = sinceProgrammatic > programmaticCameraQuietWindow
                    if viewModel.homeMode == .phoneGuidance && isLikelyUser {
                        viewModel.noteUserMapInteraction()
                    }
                }
                .onChange(of: viewModel.mapRecenterRequestID) { _, _ in
                    refreshCameraForCurrentMode()
                }
                .onChange(of: viewModel.mapFollowRiderTick) { _, _ in
                    refreshCameraForCurrentMode()
                }
                .onChange(of: viewModel.progressDistanceM) { _, _ in
                    // Spec line 102 + 101: when progress crosses a vertex,
                    // the route-segment bearing flips and the camera must
                    // re-orient. mapFollowRiderTick covers the GPS-fix
                    // case but not the "bearing changed because we crossed
                    // a corner" case during a single fix's processing.
                    refreshCameraForCurrentMode()
                }
                .onChange(of: appModel.riderLocation) { _, newValue in
                    // Spec lines 84 + 110 + 108-118: every GPS fix feeds
                    // the heading-trail buffer (so the camera can rotate
                    // to riding direction) AND bumps the follow tick
                    // (which wakes refreshCameraForCurrentMode). The
                    // viewmodel filters by mode + motion before acting.
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
                MapPolyline(coordinates: active.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                    .stroke(viewModel.homeMode == .deviceOverview ? .blue : .green, lineWidth: 7)
                ForEach(viewModel.guidanceAlternatives, id: \.id) { alt in
                    MapPolyline(coordinates: alt.normalizedPackage.geometry.map { CLLocationCoordinate2D(latitude: $0.latitude, longitude: $0.longitude) })
                        .stroke(.teal.opacity(0.45), lineWidth: 4)
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
            // Tap on the map (outside the search overlay) collapses the dropdown
            // and drops keyboard focus. Mirrors the web outside-click dismiss.
            TapGesture().onEnded {
                guard viewModel.homeMode == .planning, viewModel.isSearchOpen else { return }
                isSearchFieldFocused = false
                viewModel.closeSearch()
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

    /// Speed badge shown whenever the rider is moving (with or without an
    /// active route) — the same "moving" signal the camera uses to enter
    /// routing-anchor mode (`travelHeadingDegrees != nil`). Lives in the
    /// bottom-leading corner so it sits next to the floating Stop button
    /// during routing without overlapping it.
    @ViewBuilder
    private var speedBadge: some View {
        let moving = viewModel.travelHeadingDegrees != nil
        let inGuidance = viewModel.homeMode == .phoneGuidance
        let suggestionsVisible = viewModel.isExploringAlternativesFromGuidance ||
            (viewModel.homeMode == .planning && !appModel.preview.alternatives.isEmpty)
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

    /// On-map zoom +/- buttons. Spec line 10: "zoom + and - buttons under
    /// the top bar on the right side of screen." Adjusts the camera by a
    /// multiplicative factor each tap. In riding mode the new camera
    /// distance is persisted (`settings.ridingCameraDistanceM`) so the
    /// rider's preferred navigation zoom survives across sessions; in
    /// planning/overview the camera updates for the moment but isn't
    /// persisted (spec: "only keep it for moment").
    /// Standard size + corner radius shared by every right-side rail
    /// glyph so the column is pixel-perfect across modes.
    private static let railIconSize: CGFloat = 50
    private static let railIconCorner: CGFloat = 18
    private static let railIconSpacing: CGFloat = 8

    /// Single Y offset used by BOTH rails in EVERY mode. Sized to clear
    /// the tallest top card we ever render: the routing card with an
    /// optional off-route pill + 3-line guidance text. Keeping both
    /// rails on the same offset means rail icons never shift Y when
    /// the rider goes from planning → routing → arrival.
    ///
    /// 130 leaves daylight for a 3-line routing card (~96pt with
    /// paddings) without leaving an obvious empty band in planning mode.
    private static let railTopPadding: CGFloat = 130

    /// Persistent right-side rail, sitting below the Settings cog (which
    /// is rendered inside the where-to top bar). Items, top → bottom:
    /// compass/north-up, device chip (only when paired). Driven by
    /// `viewModel.topRightIconStack` so the unit tests pin the order.
    @ViewBuilder
    private var rightSideRailTop: some View {
        if viewModel.shouldShowSearchPanel {
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

    /// Persistent left-side rail, sitting below the where-to search bar
    /// in the same vertical band as the right rail. Items, top → bottom:
    /// alternate-routes (only in routing), zoom-in, zoom-out. Lives on
    /// the LEFT rather than the bottom-right because the suggested-routes
    /// card sits along the bottom and previously occluded the column.
    @ViewBuilder
    private var leftSideRailTop: some View {
        if viewModel.shouldShowSearchPanel {
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
            // Only reached in non-planning modes (planning routes Settings
            // to the where-to top bar). HomeViewModel.topRightIconStack
            // hides this case when there is a where-to to attach to.
            Button(action: onOpenSettings) {
                railGlyph("gearshape.fill")
            }
            .buttonStyle(.plain)
            .accessibilityLabel(T.string("home.a11y.settings"))
        case .compass:
            // Single combined glyph: tap = recenter / show north-up,
            // double-tap (quick) = lock north-up. Replaces the previous
            // separate "Recenter map" planning button + "North indicator"
            // routing button.
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
        // Match `planningMapAccessoryControls` (the north-up / recenter
        // button) and the top-right settings cog: 50×50 frame, corner
        // 18, ultraThinMaterial, primary foreground. The earlier 44/14
        // values made the +/- column visually inconsistent with the
        // existing top-right glyphs.
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
            // Riding mode: persist the new distance through the viewModel
            // (so the autoFollow camera in `orientCameraForTravel` reads the
            // same value next time GPS bumps the follow tick) AND apply it
            // directly here so there's no perceptible "no-op" window
            // between the tap and the next refresh.
            viewModel.bumpRidingZoom(direction: direction == .zoomIn ? .zoomIn : .zoomOut)
            let next = viewModel.ridingCameraDistanceM
            if let route = viewModel.guidanceRoute {
                let rider = appModel.riderLocation
                let heading = viewModel.cameraHeadingDegrees(rider: rider)
                    ?? bearingDegrees(from: route.geometry.first ?? rider,
                                      to: route.geometry.dropFirst().first ?? rider)
                // Pass the new camera distance so the anchor offset scales
                // proportionally — without this, deep zoom-in (small distance)
                // pushes the rider off the bottom of the visible map.
                let centerPoint = viewModel.cameraCenterCoordinate(rider: rider, headingDegrees: heading, cameraDistanceM: next)
                let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
                withAnimation(.easeInOut(duration: 0.25)) {
                    cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: next, heading: heading, pitch: 0))
                }
                lastProgrammaticCameraSetAt = Date()
            } else {
                refreshCameraForCurrentMode()
            }
        case .planning, .deviceOverview, .sendingToDevice:
            // Outside riding: a session-only zoom on the current camera.
            // `MapCameraPosition` is an opaque struct (not an enum) so we
            // can't read its current region directly — but `onMapCameraChange`
            // already keeps `planningCameraReference` in sync with the live
            // camera, so we use that as the source of truth and fall back
            // to the rider when no reference is available yet.
            let factor: Double = direction == .zoomIn
                ? (1.0 / HomeViewModel.ridingZoomStepFactor)
                : HomeViewModel.ridingZoomStepFactor
            let rider = appModel.riderLocation
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
            } else if viewModel.planningStatus != nil || appModel.importActivityStatus != nil {
                planningProgressCard
            } else if !viewModel.previewAlternatives.isEmpty {
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
        VStack(alignment: .leading, spacing: 6) {
            Text(message)
                .font(.headline)
            Text(T.string("home.routingFinished"))
                .font(.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
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
        // Three-line text card (no icons): next-turn headline,
        // "X km to <destination>", "Y min remaining". Icons live on the
        // persistent rails so the layout doesn't reflow between modes.
        let layout = viewModel.routingTopLayout
        return VStack(spacing: 6) {
            if let offLabel = layout?.offRouteLabel {
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
        }
    }

    private func phoneGuidanceTopCard(
        headline: String,
        distanceLine: String,
        minutesLine: String
    ) -> some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(headline)
                .font(.headline)
                .lineLimit(2)
            if !distanceLine.isEmpty {
                Text(distanceLine)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            if !minutesLine.isEmpty {
                Text(minutesLine)
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    /// Routing-mode side rail. Stacks the device-pairing chip, the
    private var deviceOverviewTopOverlay: some View {
        // Text-only top card; the device chip and settings glyph live on
        // the persistent right-side rail (which sits BELOW this card,
        // not beside it, so the card spans full width).
        VStack(alignment: .leading, spacing: 4) {
            Text(viewModel.activeNavigationTitle)
                .font(.headline)
                .lineLimit(1)
            Text(viewModel.activeNavigationSubtitle)
                .font(.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(2)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
    }

    private var topBar: some View {
        HStack(spacing: 10) {
            HStack(spacing: 12) {
                Image(systemName: "magnifyingglass")
                    .foregroundStyle(.secondary)
                TextField(T.string("home.whereTo"), text: Binding(
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
                    .accessibilityLabel(T.string("home.a11y.clearDestination"))
                }
            }
            .padding(14)
            .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
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
                if viewModel.isResolvingUrl {
                    HStack(alignment: .center, spacing: 12) {
                        ProgressView()
                        VStack(alignment: .leading, spacing: 4) {
                            Text(T.string("home.resolvingLink"))
                                .font(.headline)
                            Text(T.string("home.resolvingLinkSubtitle"))
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else if let urlError = viewModel.urlResolveError {
                    VStack(alignment: .leading, spacing: 4) {
                        Text(T.string("home.linkFailed"))
                            .font(.headline)
                        Text(urlError)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else if viewModel.query.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    ForEach(viewModel.recentItems) { item in
                        Button {
                            isSearchFieldFocused = false
                            viewModel.selectRecent(item)
                        } label: {
                            routeHistoryRow(item)
                        }
                        .buttonStyle(.plain)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
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
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .contentShape(Rectangle())
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
        .contentShape(Rectangle())
    }

    /// Planning-only status overlay. The recenter button has moved into
    /// the persistent compass glyph on the right-side rail — tapping the
    /// compass triggers the same `refreshCameraForCurrentMode` path. What
    /// remains here is the locating spinner and the location-blocked
    /// indicator, both informational.
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
        .contentShape(Rectangle())
    }

    private var planningProgressCard: some View {
        VStack(alignment: .leading, spacing: 12) {
            HStack(spacing: 12) {
                ProgressView()
                VStack(alignment: .leading, spacing: 4) {
                    Text(T.string("home.workingOnRoute"))
                        .font(.headline)
                    Text(viewModel.planningStatus ?? appModel.importActivityStatus ?? "Planning route…")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
                Spacer()
            }
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
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
                            // Drop the redundant subtitle (it was empty
                            // after the title rename anyway). Two visible
                            // lines max: title + km/min summary.
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
            .disabled(viewModel.homeMode == .sendingToDevice)
        }
        .padding(16)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 22, style: .continuous))
        .padding(.horizontal, 16)
        .padding(.bottom, 16)
    }

    private func activeGuidanceCard(_ active: NormalizedRoutePackage) -> some View {
        // The destination + remaining + ETA all live in the top card now
        // (see `guidanceSubtitleLine`). The bottom slot is intentionally
        // minimal: a floating Stop button (and the speed badge sits next
        // to it via the surrounding overlay).
        Button(role: .destructive, action: { viewModel.stopActiveNavigation() }) {
            Text(T.string("home.stop"))
                .font(.headline)
                .padding(.horizontal, 24)
        }
        .buttonStyle(.borderedProminent)
        .controlSize(.large)
        .padding(.bottom, 24)
        .padding(.horizontal, 16)
        .frame(maxWidth: .infinity, alignment: .trailing)
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
                // Fit ONLY the remaining route — long rides made overview
                // useless when the camera kept zooming out to include
                // already-completed kilometres.
                let overview = viewModel.routeOverviewGeometry
                fitCamera(to: overview.count >= 2 ? overview : route.geometry)
            }
        }
    }

    private func resetPlanningCamera() {
        let coordinates = viewModel.displayedRouteCoordinates
        // Spec lines 108-118: if the rider is moving (with or without a
        // route preview), enter riding-mode camera — bottom-quarter anchor
        // and rotate to GPS-trail heading.
        if let trailHeading = viewModel.travelHeadingDegrees {
            let rider = appModel.riderLocation
            let centerPoint = viewModel.cameraCenterCoordinate(
                rider: rider, headingDegrees: trailHeading, cameraDistanceM: ridingCameraDistanceM
            )
            let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
            // Smooth motion (#2): without `withAnimation` MapKit hard-snaps on
            // each fix and the rider visually jumps before the map catches up.
            // Duration was 0.45 s but felt laggy through tight turns — Apple
            // Maps uses something closer to 0.2 s, which matches the natural
            // cadence of GPS fixes (~1 s) without overshooting the next one.
            withAnimation(.easeInOut(duration: 0.22)) {
                cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: ridingCameraDistanceM, heading: trailHeading, pitch: 0))
            }
            lastProgrammaticCameraSetAt = Date()
            return
        }
        if !coordinates.isEmpty {
            fitCamera(to: coordinates, recordPlanningReference: true)
        } else {
            let rider = appModel.riderLocation
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
        // Spec line 110 wins over 101 when the rider is moving: rotate to
        // the GPS-trail heading. `cameraHeadingDegrees` returns trail-first
        // and falls back to the route-segment bearing when stationary.
        let rider = appModel.riderLocation
        let heading = viewModel.cameraHeadingDegrees(rider: rider)
            ?? bearingDegrees(from: route.geometry[0], to: route.geometry[1])
        // Spec line 84: anchor rider in the bottom quarter. iOS MapKit has
        // no padding-based anchor offset; instead we shift the camera
        // center ahead of the rider in the heading direction so the rider
        // renders visually below center. Pass the same camera distance
        // we're about to use so the offset scales with zoom — without
        // this, a deep zoom-in puts the rider off the bottom edge.
        let centerPoint = viewModel.cameraCenterCoordinate(
            rider: rider, headingDegrees: heading, cameraDistanceM: ridingCameraDistanceM
        )
        let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
        // Smooth motion (#2): wrap programmatic camera moves so SwiftUI Map
        // animates the transition instead of snapping. Pinned a hair under
        // the GPS cadence (≥1 s typical) so successive fixes can interrupt
        // the previous animation without queueing.
        //
        // Distance must come from the viewModel — NOT a hardcoded constant.
        // The +/- buttons write through `viewModel.bumpRidingZoom` and the
        // autoFollow camera reads that same value here, which is what keeps
        // the rider's preferred zoom alive across compass cycles + every
        // GPS-fix-driven refresh.
        withAnimation(.easeInOut(duration: 0.22)) {
            cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: ridingCameraDistanceM, heading: heading, pitch: 0))
        }
        lastProgrammaticCameraSetAt = Date()
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

        let region = HomeViewModel.fittedRouteRegion(
            minLat: minLat, maxLat: maxLat,
            minLon: minLon, maxLon: maxLon
        )
        setCamera(region: region, heading: 0, recordPlanningReference: recordPlanningReference)
    }

    private func setCamera(region: MKCoordinateRegion, heading: CLLocationDirection, recordPlanningReference: Bool) {
        // Animate region changes (overview fits, planning recenter) so the
        // map moves smoothly rather than snapping. Same envelope as the
        // follow-rider path so cadence feels uniform.
        withAnimation(.easeInOut(duration: 0.45)) {
            cameraPosition = .region(region)
        }
        lastProgrammaticCameraSetAt = Date()
        if recordPlanningReference {
            planningCameraReference = PlanningCameraReference(center: region.center, span: region.span, heading: heading)
            planningCameraNeedsReset = false
        }
    }

    private func updatePlanningCameraResetState(for region: MKCoordinateRegion, heading: CLLocationDirection) {
        guard viewModel.homeMode == .planning, let reference = planningCameraReference else {
            planningCameraNeedsReset = false
            return
        }

        let centerLatDelta = abs(region.center.latitude - reference.center.latitude)
        let centerLonDelta = abs(region.center.longitude - reference.center.longitude)
        let spanLatDelta = abs(region.span.latitudeDelta - reference.span.latitudeDelta)
        let spanLonDelta = abs(region.span.longitudeDelta - reference.span.longitudeDelta)
        let normalizedHeading = abs(heading.truncatingRemainder(dividingBy: 360))

        planningCameraNeedsReset =
            centerLatDelta > 0.0003 ||
            centerLonDelta > 0.0003 ||
            spanLatDelta > 0.002 ||
            spanLonDelta > 0.002 ||
            normalizedHeading > 4
    }

    private func bearingDegrees(from start: CoordinatePoint, to end: CoordinatePoint) -> CLLocationDirection {
        let latMeters = (end.latitude - start.latitude) * 111_320.0
        let lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * .pi / 180.0) * 111_320.0
        return atan2(lonMeters, latMeters) * 180.0 / .pi
    }
}
