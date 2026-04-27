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
        if moving || inGuidance {
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
            .accessibilityLabel("Current speed")
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
            if let active = viewModel.guidanceRoute {
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
            Text("Routing finished. Tap a destination to plan again.")
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
        // Top card: next-turn line as the headline, destination + remaining
        // bundled as the subtitle (single source of truth — the bottom just
        // floats a stop button). See HomeViewModel.guidanceSubtitleLine.
        let headline = viewModel.nextInstructionLine ?? viewModel.activeNavigationTitle
        return HStack(alignment: .center, spacing: 12) {
            VStack(alignment: .leading, spacing: 4) {
                Text(headline)
                    .font(.headline)
                    .lineLimit(1)
                Text(viewModel.guidanceSubtitleLine)
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
                if viewModel.isResolvingUrl {
                    HStack(alignment: .center, spacing: 12) {
                        ProgressView()
                        VStack(alignment: .leading, spacing: 4) {
                            Text("Resolving link…")
                                .font(.headline)
                            Text("Following the URL to a destination. This can take a couple of seconds.")
                                .font(.caption)
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                    }
                    .padding(14)
                    .frame(maxWidth: .infinity, alignment: .leading)
                } else if let urlError = viewModel.urlResolveError {
                    VStack(alignment: .leading, spacing: 4) {
                        Text("Couldn't open that link")
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

    @ViewBuilder
    private var planningMapAccessoryControls: some View {
        if viewModel.homeMode == .planning {
            if isWaitingForFirstLocationFix {
                ProgressView()
                    .progressViewStyle(.circular)
                    .controlSize(.small)
                    .frame(width: 50, height: 50)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .accessibilityLabel("Locating you")
                    .padding(.trailing, 16)
                    .padding(.top, 72)
            } else if appModel.locationService.lastError == .denied {
                Image(systemName: "location.slash.fill")
                    .font(.system(size: 18, weight: .semibold))
                    .frame(width: 50, height: 50)
                    .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                    .accessibilityLabel("Location is blocked")
                    .padding(.trailing, 16)
                    .padding(.top, 72)
            } else if planningCameraNeedsReset {
                Button(action: refreshCameraForCurrentMode) {
                    Image(systemName: "location.north.line.fill")
                        .font(.system(size: 18, weight: .semibold))
                        .frame(width: 50, height: 50)
                        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 18, style: .continuous))
                }
                .accessibilityLabel("Recenter map")
                .padding(.trailing, 16)
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
                    Text("Working on route")
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
        // The destination + remaining + ETA all live in the top card now
        // (see `guidanceSubtitleLine`). The bottom slot is intentionally
        // minimal: a floating Stop button (and the speed badge sits next
        // to it via the surrounding overlay).
        Button(role: .destructive, action: viewModel.stopActiveNavigation) {
            Text("Stop")
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
            let centerPoint = viewModel.cameraCenterCoordinate(rider: rider, headingDegrees: trailHeading)
            let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
            cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: 1200, heading: trailHeading, pitch: 0))
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
        // renders visually below center.
        let centerPoint = viewModel.cameraCenterCoordinate(rider: rider, headingDegrees: heading)
        let center = CLLocationCoordinate2D(latitude: centerPoint.latitude, longitude: centerPoint.longitude)
        cameraPosition = .camera(MapCamera(centerCoordinate: center, distance: 1200, heading: heading, pitch: 0))
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

        let latDelta = max((maxLat - minLat) * 1.5, 0.01)
        let lonDelta = max((maxLon - minLon) * 1.5, 0.01)
        let center = CLLocationCoordinate2D(latitude: (minLat + maxLat) / 2.0, longitude: (minLon + maxLon) / 2.0)
        setCamera(
            region: MKCoordinateRegion(center: center, span: MKCoordinateSpan(latitudeDelta: latDelta, longitudeDelta: lonDelta)),
            heading: 0,
            recordPlanningReference: recordPlanningReference
        )
    }

    private func setCamera(region: MKCoordinateRegion, heading: CLLocationDirection, recordPlanningReference: Bool) {
        cameraPosition = .region(region)
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
