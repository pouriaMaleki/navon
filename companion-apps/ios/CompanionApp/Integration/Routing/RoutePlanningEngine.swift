import Foundation

/// Provider-agnostic route planning: alternative selection, merging, labelling, and preview building.
enum RoutePlanningEngine {
    static func presentAlternatives(_ alternatives: [RouteAlternative], sourceMode: RouteSourceMode) -> [RouteAlternative] {
        alternatives.prefix(3).map { alternative in
            let label = friendlyAlternativeLabel(for: alternative)
            return RouteAlternative(
                id: alternative.id,
                title: label.title,
                subtitle: label.subtitle,
                distanceMeters: alternative.distanceMeters,
                durationSeconds: alternative.durationSeconds,
                normalizedPackage: alternative.normalizedPackage
            )
        }
    }

    static func friendlyAlternativeLabel(for alternative: RouteAlternative) -> (title: String, subtitle: String) {
        let providerID = alternative.normalizedPackage.provenance.providerID
        let sourceRef = alternative.normalizedPackage.provenance.sourceReference?.lowercased() ?? ""
        switch providerID {
        case .osm:
            if sourceRef.contains("fastbike") { return ("BRouter fastbike", "") }
            if sourceRef.contains("trekking") { return ("BRouter trekking", "") }
            return ("OSM Route", "")
        case .hsl:
            if sourceRef.contains("fastest") { return ("HSL Fastest", "") }
            return ("HSL Route", "")
        case .gpxImport, .fitImport, .tcxImport:
            return (providerID.displayName, "")
        }
    }
    static func mergeMixedAlternatives(_ alternatives: [RouteAlternative]) -> [RouteAlternative] {
        guard !alternatives.isEmpty else { return [] }
        var remaining = alternatives.sorted {
            if $0.durationSeconds == $1.durationSeconds {
                return $0.distanceMeters < $1.distanceMeters
            }
            return $0.durationSeconds < $1.durationSeconds
        }

        var chosen: [RouteAlternative] = []
        if let fastest = remaining.first {
            chosen.append(fastest)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == fastest.normalizedPackage.routeIdentifier }
        }

        if let quieter = remaining.first(where: { $0.normalizedPackage.provenance.providerID == .osm }) ?? remaining.first {
            chosen.append(quieter)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == quieter.normalizedPackage.routeIdentifier }
        }

        if let simpler = remaining.min(by: { $0.normalizedPackage.maneuverCount < $1.normalizedPackage.maneuverCount }) {
            chosen.append(simpler)
            remaining.removeAll { $0.normalizedPackage.routeIdentifier == simpler.normalizedPackage.routeIdentifier }
        }

        while chosen.count < 3, let next = remaining.first {
            chosen.append(next)
            remaining.removeFirst()
        }

        return presentAlternatives(chosen, sourceMode: .mixed)
    }

    static func preferredMixedPreviews(from previews: [RoutePreviewModel]) -> [RoutePreviewModel] {
        previews.filter { !$0.alternatives.isEmpty }
    }

    static func mixedPlanningNotice(from previews: [RoutePreviewModel], effectivePreviews: [RoutePreviewModel]) -> String {
        if effectivePreviews.count == 1, let notice = effectivePreviews.first?.planningNotice, !notice.isEmpty {
            return notice
        }
        if effectivePreviews.count < previews.count {
            return "Showing available routes while some providers are hidden."
        }
        return T.string("planning.mixedRoutesFromHslAndOsm")
    }
    static func displayDestinationTitle(selectedPackage: NormalizedRoutePackage?, preferredTitle: String?, fallback: String) -> String {
        if let preferredTitle {
            let trimmed = preferredTitle.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty { return trimmed }
        }
        if let packageTitle = selectedPackage?.summary.destinationLabel,
           !isGenericDestinationTitle(packageTitle, providerID: selectedPackage?.provenance.providerID) {
            return packageTitle
        }
        return fallback
    }

    static func isGenericDestinationTitle(_ title: String, providerID: RouteProviderID?) -> Bool {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty { return true }
        let lowercased = trimmed.lowercased()
        if lowercased == "selected destination" || lowercased == "route" || lowercased == "recent destination" || lowercased == "dropped pin" {
            return true
        }
        if let providerID, lowercased == "\(providerID.displayName.lowercased()) sample destination" {
            return true
        }
        return false
    }
    static func buildPreview(
        for request: RoutePlanRequest,
        sourceMode: RouteSourceMode,
        providers: [RouteProviderID: RoutingProvider],
        isHslAvailable: Bool,
        currentSourceMode: RouteSourceMode,
        session: ActiveRouteSession,
        revisionOverride: Int?,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {
        switch sourceMode {
        case .mixed:
            return await buildMixedPreview(
                for: request,
                providers: providers,
                isHslAvailable: isHslAvailable,
                currentSourceMode: currentSourceMode,
                session: session,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
        case .hsl, .osm:
            guard let provider = providers[sourceMode.primaryProviderID] else {
                throw NSError(domain: "RoutePlanningEngine", code: 1, userInfo: [NSLocalizedDescriptionKey: "Missing provider for \(sourceMode.displayName)"])
            }
            var preview = try await self.preview(
                from: provider,
                request: request,
                currentSourceMode: currentSourceMode,
                session: session,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
            preview.alternatives = presentAlternatives(preview.alternatives, sourceMode: sourceMode)
            preview.selectedAlternativeID = preview.alternatives.first?.id
            preview.routeIdentifier = preview.alternatives.first?.normalizedPackage.routeIdentifier
            preview.routeRevision = preview.alternatives.first?.normalizedPackage.revision
            return preview
        }
    }

    private static func preview(
        from provider: RoutingProvider,
        request: RoutePlanRequest,
        currentSourceMode: RouteSourceMode,
        session: ActiveRouteSession,
        revisionOverride: Int?,
        rerouteContext: RerouteContext?
    ) async throws -> RoutePreviewModel {
        if let revisionOverride {
            let replanSession = ActiveRouteSession(
                routeIdentifier: session.routeIdentifier,
                routeRevision: revisionOverride - 1,
                destinationLabel: session.destinationLabel,
                destinationCoordinate: request.destination,
                providerID: provider.providerID,
                sourceMode: currentSourceMode,
                lastRerouteReason: session.lastRerouteReason,
                lastRerouteTimestamp: session.lastRerouteTimestamp
            )
            return try await provider.replanRoute(
                using: replanSession,
                riderLocation: request.origin,
                rerouteContext: rerouteContext
            )
        }
        return try await provider.planRoute(request)
    }

    private static func buildMixedPreview(
        for request: RoutePlanRequest,
        providers: [RouteProviderID: RoutingProvider],
        isHslAvailable: Bool,
        currentSourceMode: RouteSourceMode,
        session: ActiveRouteSession,
        revisionOverride: Int?,
        rerouteContext: RerouteContext?
    ) async -> RoutePreviewModel {
        guard let osm = providers[.osm] else {
            return RoutePreviewModel(alternatives: [], planningNotice: "Mixed mode providers are unavailable")
        }
        let includeHsl = isHslAvailable && providers[.hsl] != nil
        var tasks: [Task<RoutePreviewModel, Never>] = [
            Task { await fetchMixedProvider(osm, request: request, currentSourceMode: currentSourceMode, session: session, revisionOverride: revisionOverride, rerouteContext: rerouteContext) },
        ]
        if includeHsl {
            tasks.append(Task { await fetchMixedProvider(providers[.hsl]!, request: request, currentSourceMode: currentSourceMode, session: session, revisionOverride: revisionOverride, rerouteContext: rerouteContext) })
        }
        var previews: [RoutePreviewModel] = []
        for task in tasks {
            previews.append(await task.value)
        }
        let effectivePreviews = preferredMixedPreviews(from: previews)
        let merged = mergeMixedAlternatives(effectivePreviews.flatMap(\.alternatives))
        return RoutePreviewModel(
            alternatives: merged,
            selectedAlternativeID: merged.first?.id,
            routeIdentifier: merged.first?.normalizedPackage.routeIdentifier,
            routeRevision: merged.first?.normalizedPackage.revision,
            planningNotice: mixedPlanningNotice(from: previews, effectivePreviews: effectivePreviews)
        )
    }

    /// Fetch from a single provider for mixed mode, never throwing — failures return
    /// an empty preview so other providers can still contribute.
    private static func fetchMixedProvider(
        _ provider: RoutingProvider,
        request: RoutePlanRequest,
        currentSourceMode: RouteSourceMode,
        session: ActiveRouteSession,
        revisionOverride: Int?,
        rerouteContext: RerouteContext?
    ) async -> RoutePreviewModel {
        do {
            return try await preview(
                from: provider,
                request: request,
                currentSourceMode: currentSourceMode,
                session: session,
                revisionOverride: revisionOverride,
                rerouteContext: rerouteContext
            )
        } catch {
            return RoutePreviewModel(alternatives: [], planningNotice: "\(provider.providerID.rawValue) unavailable: \(error.localizedDescription)")
        }
    }
}
