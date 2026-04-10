import Foundation

extension AppModel {
    var importDiagnosticsEntries: [ImportDiagnosticsEntry] {
        persistence.loadImportDiagnostics()
    }

    func dismissImportDiagnosticsEntry(id: String) {
        persistence.dismissImportDiagnosticsEntry(id: id)
        notePersistenceChanged()
    }

    func retrySharedImport(_ entry: ImportDiagnosticsEntry, using searchService: PlaceSearchService = MapKitPlaceSearchService()) async {
        var retriedEnvelope = entry.envelope
        retriedEnvelope.id = UUID().uuidString
        await handleSharedImport(retriedEnvelope, using: searchService)
        persistence.dismissImportDiagnosticsEntry(id: entry.id)
        notePersistenceChanged()
    }

    func consumePendingSharedImports(using searchService: PlaceSearchService = MapKitPlaceSearchService()) async {
        let envelopes = SharedImportStore().drainQueue()
        guard !envelopes.isEmpty else { return }
        for envelope in envelopes {
            await handleSharedImport(envelope, using: searchService)
        }
    }

    func ingestSharedImport(_ envelope: SharedImportEnvelope, using searchService: PlaceSearchService = MapKitPlaceSearchService()) async {
        await handleSharedImport(envelope, using: searchService)
    }

    private func handleSharedImport(_ envelope: SharedImportEnvelope, using searchService: PlaceSearchService) async {
        let resolved = await classifySharedImport(envelope, using: searchService)
        switch resolved.disposition {
        case .directHomePreview:
            if resolved.classification == .gpxFile, let filePath = resolved.storedFilePath {
                await importSharedGpxFile(atPath: filePath, sourceLabel: resolved.sourceApplication ?? "Shared GPX")
            } else if let destination = resolvedDestination(from: resolved) {
                await planSharedDestinationImport(destination, from: resolved)
            } else {
                saveImportDiagnostic(for: resolved)
            }
        case .routeDetailReview, .diagnosticsOnly:
            saveImportDiagnostic(for: resolved)
        }
    }

    private func importSharedGpxFile(atPath path: String, sourceLabel: String) async {
        await importGpxFile(from: URL(fileURLWithPath: path))
        recordPlannedPreview(source: .gpxImport, sourceLabel: sourceLabel)
        homePreviewRequestID = UUID()
    }

    private func planSharedDestinationImport(_ destination: DestinationSearchResult, from envelope: SharedImportEnvelope) async {
        routeRequest = RoutePlanRequest(
            origin: simulatedRiderLocation,
            destination: destination.coordinate,
            providerID: currentSourceMode.primaryProviderID
        )
        await planRoute(using: currentSourceMode, preferredTitle: destination.title)
        recordRecentDestination(title: destination.title, coordinate: destination.coordinate)
        let historySource: RouteHistorySource = envelope.classification == .googleMapsLocationLink ? .googleMaps : .shareImport
        let sourceLabel = envelope.classification == .googleMapsLocationLink ? "Google Maps" : "Shared"
        recordImportedPreview(title: destination.title, source: historySource, sourceLabel: sourceLabel)
        homePreviewRequestID = UUID()
    }

    private func recordImportedPreview(title: String, source: RouteHistorySource, sourceLabel: String) {
        guard let selected = preview.selectedAlternative?.normalizedPackage else { return }
        persistence.saveRouteHistoryItem(
            RouteHistoryItem(
                id: selected.routeIdentifier,
                title: title,
                subtitle: selected.summaryLine,
                source: source,
                sourceLabel: sourceLabel,
                createdAt: Date(),
                destination: selected.geometry.last,
                routePackage: selected,
                occurrenceCount: nil
            )
        )
        notePersistenceChanged()
    }

    private func saveImportDiagnostic(for envelope: SharedImportEnvelope) {
        persistence.saveImportDiagnosticsEntry(
            ImportDiagnosticsEntry(
                id: envelope.id,
                envelope: envelope,
                createdAt: Date()
            )
        )
        notePersistenceChanged()
    }

    private func resolvedDestination(from envelope: SharedImportEnvelope) -> DestinationSearchResult? {
        guard let coordinate = extractCoordinate(from: envelope.originalURL ?? envelope.originalText ?? "") else { return nil }
        let title = extractedTitle(from: envelope) ?? (envelope.classification == .googleMapsLocationLink ? "Imported from Google Maps" : "Shared location")
        let subtitle = envelope.originalURL ?? envelope.originalText ?? ""
        return DestinationSearchResult(id: envelope.id, title: title, subtitle: subtitle, coordinate: coordinate)
    }

    private func classifySharedImport(_ envelope: SharedImportEnvelope, using searchService: PlaceSearchService) async -> SharedImportEnvelope {
        if let path = envelope.storedFilePath ?? envelope.originalURL, envelope.classification == .gpxFile || path.lowercased().hasSuffix(".gpx") {
            var resolved = envelope
            resolved.classification = .gpxFile
            resolved.disposition = .directHomePreview
            return resolved
        }

        let payload = envelope.originalURL ?? envelope.originalText ?? ""
        if let url = extractURL(from: payload), isGoogleMapsURL(url) {
            var resolved = envelope
            resolved.originalURL = url
            resolved.classification = .googleMapsLocationLink
            if extractCoordinate(from: url) != nil {
                resolved.disposition = .directHomePreview
                return resolved
            }
            if let resolvedDestination = await resolveTextDestination(from: envelope, using: searchService) {
                var destinationEnvelope = resolved
                destinationEnvelope.originalText = "\(resolvedDestination.title)\n\(resolvedDestination.coordinate.latitude),\(resolvedDestination.coordinate.longitude)"
                destinationEnvelope.disposition = .directHomePreview
                return destinationEnvelope
            }
            resolved.disposition = .diagnosticsOnly
            resolved.note = "Google Maps link could not be resolved confidently yet."
            return resolved
        }

        if let url = extractURL(from: payload), extractCoordinate(from: url) != nil {
            var resolved = envelope
            resolved.originalURL = url
            resolved.classification = .genericLocationLink
            resolved.disposition = .directHomePreview
            return resolved
        }

        if extractCoordinate(from: payload) != nil {
            var resolved = envelope
            resolved.classification = .plainCoordinates
            resolved.disposition = .directHomePreview
            return resolved
        }

        var resolved = envelope
        resolved.classification = .unsupportedUnknown
        resolved.disposition = .diagnosticsOnly
        if resolved.note == nil {
            resolved.note = "Shared item type is not supported yet."
        }
        return resolved
    }

    private func resolveTextDestination(from envelope: SharedImportEnvelope, using searchService: PlaceSearchService) async -> DestinationSearchResult? {
        guard let text = extractedTitle(from: envelope), !text.isEmpty else { return nil }
        return await searchService.searchDestinations(matching: text, limit: 1).first
    }

    private func extractedTitle(from envelope: SharedImportEnvelope) -> String? {
        let text = envelope.originalText ?? envelope.originalURL ?? ""
        let lines = text
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
        if let first = lines.first, !first.starts(with: "http://"), !first.starts(with: "https://") {
            return first
        }
        if let url = envelope.originalURL,
           let queryItems = URLComponents(string: url)?.queryItems,
           let name = queryItems.first(where: { ["q", "query", "destination"].contains($0.name.lowercased()) })?.value,
           !name.isEmpty,
           extractCoordinate(from: name) == nil {
            return name.replacingOccurrences(of: "+", with: " ")
        }
        return nil
    }

    private func extractURL(from value: String) -> String? {
        value
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .first(where: { $0.starts(with: "http://") || $0.starts(with: "https://") })
    }

    private func isGoogleMapsURL(_ value: String) -> Bool {
        guard let host = URL(string: value)?.host?.lowercased() else { return false }
        return host.contains("google.") || host == "maps.app.goo.gl" || host == "goo.gl"
    }

    private func extractCoordinate(from value: String) -> CoordinatePoint? {
        let pattern = try? NSRegularExpression(pattern: "(-?\\d{1,3}\\.\\d+)[,\\s]+(-?\\d{1,3}\\.\\d+)")
        guard let match = pattern?.firstMatch(in: value, range: NSRange(location: 0, length: value.utf16.count)),
              let latitudeRange = Range(match.range(at: 1), in: value),
              let longitudeRange = Range(match.range(at: 2), in: value),
              let latitude = Double(value[latitudeRange]),
              let longitude = Double(value[longitudeRange]),
              (-90.0...90.0).contains(latitude),
              (-180.0...180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }
}
