import Foundation

/// Result of asking the share-import classifier to follow a URL to a destination.
enum UrlDestinationResolution {
    case coordinate(CoordinatePoint, suggestedTitle: String?)
    case noDestinationFound
    case networkError(String)
}

extension AppModel {
    private struct RemotePageSummary {
        let finalURL: URL
        let pageTitle: String?
        let coordinate: CoordinatePoint?
    }

    /// Public entry point used by the in-app "Where to?" paste flow. Re-uses the same
    /// canonicalisation, redirect-following, and coordinate extraction the share extension
    /// already does, then surfaces a simple result type.
    func resolveDestinationFromUrl(
        _ urlString: String,
        using searchService: PlaceSearchService = MapKitPlaceSearchService()
    ) async -> UrlDestinationResolution {
        let trimmed = urlString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let parsed = URL(string: trimmed) else { return .noDestinationFound }

        let canonical = ShareImportUtilities.canonicalSharedURL(parsed)
        let expanded = await ShareImportUtilities.expandedSharedURL(for: canonical) ?? canonical
        let resolved = ShareImportUtilities.canonicalSharedURL(expanded)

        if let inline = ShareImportUtilities.extractCoordinate(from: resolved) {
            return .coordinate(inline, suggestedTitle: nil)
        }

        let remote = await ShareImportUtilities.remotePageSummary(for: resolved)
        if let coordinate = remote?.coordinate {
            return .coordinate(coordinate, suggestedTitle: remote?.pageTitle)
        }
        if let title = remote?.pageTitle, !title.isEmpty {
            let matches = await searchService.searchDestinations(matching: title, limit: 1)
            if let first = matches.first {
                return .coordinate(first.coordinate, suggestedTitle: first.title)
            }
        }
        return .noDestinationFound
    }

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
        retriedEnvelope = markDebugPhase(retriedEnvelope, phase: "app.retry", outcome: "started")
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
        importActivityStatus = "Importing shared item…"
        defer { importActivityStatus = nil }
        var resolved = markDebugPhase(envelope, phase: "app.handle", outcome: "started")
        resolved = await classifySharedImport(resolved, using: searchService)
        resolved = markDebugPhase(
            resolved,
            phase: "app.classify",
            outcome: "\(resolved.classification.rawValue):\(resolved.disposition.rawValue)"
        )
        switch resolved.disposition {
        case .directHomePreview:
            await resetCurrentRouteForSharedImport()
            if resolved.classification == .gpxFile, let filePath = resolved.storedFilePath {
                importActivityStatus = "Importing shared route…"
                await importSharedGpxFile(atPath: filePath, sourceLabel: resolved.sourceApplication ?? "Shared GPX", envelope: resolved)
            } else if let providerID = sharedFileProviderID(for: resolved), let filePath = resolved.storedFilePath {
                importActivityStatus = "Importing \(providerID.displayName)…"
                if let errorMessage = await importSharedSampleFile(
                    atPath: filePath,
                    providerID: providerID,
                    preferredTitle: sharedImportTitle(for: resolved),
                    sourceLabel: sharedImportSourceLabel(for: resolved),
                    envelope: resolved
                ) {
                    var diagnostic = resolved
                    diagnostic.disposition = .diagnosticsOnly
                    diagnostic.note = "\(providerID.displayName) import failed: \(errorMessage)"
                    diagnostic = markDebugPhase(diagnostic, phase: "app.sample-import", outcome: "failed:\(errorMessage)")
                    saveImportDiagnostic(for: diagnostic)
                }
            } else if let destination = await resolvedDestination(from: resolved, using: searchService) {
                importActivityStatus = "Planning route to \(destination.title)…"
                await planSharedDestinationImport(destination, from: resolved)
            } else {
                let failed = markDebugPhase(resolved, phase: "app.resolve-destination", outcome: "no-coordinate")
                saveImportDiagnostic(for: failed)
            }
        case .routeDetailReview, .diagnosticsOnly:
            saveImportDiagnostic(for: resolved)
        }
    }

    private func resetCurrentRouteForSharedImport() async {
        preview = RoutePreviewModel(alternatives: [], selectedAlternativeID: nil, routeIdentifier: nil, routeRevision: nil, planningNotice: nil)
        if isDeviceConnected, activeSession.routeIdentifier != nil {
            _ = await clearActiveRoute()
        }
        activeSession.routeIdentifier = nil
        activeSession.routeRevision = nil
        activeSession.destinationLabel = "No destination"
        activeSession.destinationCoordinate = nil
        activeSession.lastRerouteReason = nil
        activeSession.lastRerouteTimestamp = nil
        persistence.saveSession(activeSession)
        refreshDiagnostics()
    }

    private func importSharedGpxFile(atPath path: String, sourceLabel: String, envelope: SharedImportEnvelope) async {
        await importGpxFile(from: URL(fileURLWithPath: path))
        if let item = recordPlannedPreview(source: .gpxImport, sourceLabel: sourceLabel) {
            let debugTrail = (markDebugPhase(envelope, phase: "app.gpx-import", outcome: "preview-ready").debugTrail ?? []) + ["source=shared-gpx"]
            savePendingHomeImportPresentation(item: item, debugTrail: debugTrail)
        }
        homePreviewRequestID = UUID()
    }

    private func importSharedSampleFile(
        atPath path: String,
        providerID: RouteProviderID,
        preferredTitle: String,
        sourceLabel: String,
        envelope: SharedImportEnvelope
    ) async -> String? {
        do {
            try await importSampleFile(
                from: URL(fileURLWithPath: path),
                providerID: providerID,
                preferredTitle: preferredTitle
            )
            if let item = recordImportedPreview(title: preferredTitle, source: .shareImport, sourceLabel: sourceLabel) {
                let debugTrail = (markDebugPhase(envelope, phase: "app.sample-import", outcome: "preview-ready").debugTrail ?? []) + ["source=shared-\(providerID.rawValue)"]
                savePendingHomeImportPresentation(item: item, debugTrail: debugTrail)
            }
            homePreviewRequestID = UUID()
            return nil
        } catch {
            return displayShareImportError(error)
        }
    }

    private func planSharedDestinationImport(_ destination: DestinationSearchResult, from envelope: SharedImportEnvelope) async {
        routeRequest = RoutePlanRequest(
            origin: riderLocation,
            destination: destination.coordinate,
            providerID: currentSourceMode.primaryProviderID
        )
        await planRoute(using: currentSourceMode, preferredTitle: destination.title)
        recordRecentDestination(title: destination.title, coordinate: destination.coordinate)
        let historySource: RouteHistorySource = envelope.classification == .googleMapsLocationLink ? .googleMaps : .shareImport
        let sourceLabel = envelope.classification == .googleMapsLocationLink ? "Google Maps" : "Shared"
        if let item = recordImportedPreview(title: destination.title, source: historySource, sourceLabel: sourceLabel) {
            let debugTrail = markDebugPhase(
                envelope,
                phase: "app.destination-import",
                outcome: "preview-ready:\(preview.alternatives.count)-alternatives"
            ).debugTrail ?? []
            savePendingHomeImportPresentation(item: item, debugTrail: debugTrail)
        }
        homePreviewRequestID = UUID()
    }

    @discardableResult
    private func recordImportedPreview(title: String, source: RouteHistorySource, sourceLabel: String) -> RouteHistoryItem? {
        guard let selected = preview.selectedAlternative?.normalizedPackage else { return nil }
        let item = RouteHistoryItem(
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
        persistence.saveRouteHistoryItem(item)
        notePersistenceChanged()
        return item
    }

    private func saveImportDiagnostic(for envelope: SharedImportEnvelope) {
        let diagnosticEnvelope = markDebugPhase(envelope, phase: "app.diagnostics", outcome: "saved")
        persistence.saveImportDiagnosticsEntry(
            ImportDiagnosticsEntry(
                id: diagnosticEnvelope.id,
                envelope: diagnosticEnvelope,
                createdAt: Date()
            )
        )
        notePersistenceChanged()
    }

    private func markDebugPhase(_ envelope: SharedImportEnvelope, phase: String, outcome: String) -> SharedImportEnvelope {
        var updated = envelope
        updated.debugTrail = (updated.debugTrail ?? []) + ["\(phase)=\(outcome)"]
        var context = updated.debugContext ?? SharedImportDebugContext()
        let bundle = Bundle.main
        context.latestHandlerTarget = "companion-app"
        context.latestHandlerBundleID = bundle.bundleIdentifier
        context.latestHandlerVersion = bundle.infoDictionary?["CFBundleShortVersionString"] as? String
        context.latestHandlerBuild = bundle.infoDictionary?["CFBundleVersion"] as? String
        context.latestPhase = phase
        context.latestOutcome = outcome
        context.lastUpdatedAt = Date()
        updated.debugContext = context
        return updated
    }

    private func resolvedDestination(from envelope: SharedImportEnvelope, using searchService: PlaceSearchService) async -> DestinationSearchResult? {
        let coordinate = [envelope.originalText, envelope.originalURL]
            .compactMap { $0 }
            .compactMap { ShareImportUtilities.extractCoordinate(from: $0) }
            .first
        guard let coordinate else { return nil }
        let fallbackTitle = sharedImportTitle(for: envelope)
        let resolvedDestination = await searchService.resolveDestination(
            at: coordinate,
            fallbackTitle: fallbackTitle,
            preserveFallbackTitle: false
        )
        return resolvedDestination ?? DestinationSearchResult(
            id: envelope.id,
            title: fallbackTitle,
            subtitle: envelope.originalURL ?? envelope.originalText ?? "",
            coordinate: coordinate
        )
    }

    private func classifySharedImport(_ envelope: SharedImportEnvelope, using searchService: PlaceSearchService) async -> SharedImportEnvelope {
        if let resolvedFileEnvelope = classifySharedFileEnvelope(envelope) {
            return resolvedFileEnvelope
        }

        let payload = envelope.originalURL ?? envelope.originalText ?? ""
        if let urlString = ShareImportUtilities.extractURL(from: payload), let parsedURL = URL(string: urlString) {
            if let resolvedURLImport = await resolveURLImport(envelope: envelope, parsedURL: parsedURL, using: searchService) {
                return resolvedURLImport
            }
        }

        if ShareImportUtilities.extractCoordinate(from: payload) != nil {
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

    private func classifySharedFileEnvelope(_ envelope: SharedImportEnvelope) -> SharedImportEnvelope? {
        guard let path = envelope.storedFilePath ?? envelope.originalURL else { return nil }
        let lowerPath = path.lowercased()
        var resolved = envelope
        if envelope.classification == .gpxFile || lowerPath.hasSuffix(".gpx") {
            resolved.classification = .gpxFile
            resolved.disposition = .directHomePreview
            return resolved
        }
        if envelope.classification == .fitFile || lowerPath.hasSuffix(".fit") {
            resolved.classification = .fitFile
            resolved.disposition = .directHomePreview
            resolved.note = "Using sample FIT import preview until live file parsing is added."
            return resolved
        }
        if envelope.classification == .tcxFile || lowerPath.hasSuffix(".tcx") {
            resolved.classification = .tcxFile
            resolved.disposition = .directHomePreview
            resolved.note = "Using sample TCX import preview until live file parsing is added."
            return resolved
        }
        return nil
    }

    private func resolveURLImport(
        envelope: SharedImportEnvelope,
        parsedURL: URL,
        using searchService: PlaceSearchService
    ) async -> SharedImportEnvelope? {
        let canonicalURL = ShareImportUtilities.canonicalSharedURL(parsedURL)
        let expandedURL = await ShareImportUtilities.expandedSharedURL(for: canonicalURL) ?? canonicalURL
        let resolvedURL = ShareImportUtilities.canonicalSharedURL(expandedURL)

        var resolved = envelope
        resolved.debugTrail = (resolved.debugTrail ?? []) + [
            "raw-url=\(parsedURL.absoluteString)",
            "canonical-url=\(canonicalURL.absoluteString)",
            "expanded-url=\(expandedURL.absoluteString)",
            "resolved-url=\(resolvedURL.absoluteString)"
        ]
        resolved.originalURL = resolvedURL.absoluteString
        if ShareImportUtilities.isGoogleMapsURL(resolved.originalURL ?? "") {
            resolved.classification = .googleMapsLocationLink
        } else if ShareImportUtilities.isSupportedSharedURL(resolvedURL) {
            resolved.classification = .genericLocationLink
            resolved.debugTrail?.append("url-classification=genericLocationLink")
        } else {
            resolved.debugTrail?.append("url-classification=unsupported")
            return nil
        }

        if resolved.classification == .googleMapsLocationLink {
            resolved.debugTrail?.append("url-classification=googleMapsLocationLink")
            return await resolveGoogleMapsImport(resolved, url: resolvedURL, using: searchService)
        }

        let garminCourseAttempt = await garminCourseFileEnvelope(from: resolvedURL, envelope: resolved)
        resolved.debugTrail = (resolved.debugTrail ?? []) + garminCourseAttempt.debugTrail
        if let garminCourseEnvelope = garminCourseAttempt.envelope {
            return garminCourseEnvelope
        }

        if let coordinate = ShareImportUtilities.extractCoordinate(from: resolvedURL) {
            resolved.debugTrail?.append("generic-coordinate=\(coordinate.latitude),\(coordinate.longitude)")
            return await resolvedEnvelopeForCoordinate(
                resolved,
                coordinate: coordinate,
                using: searchService,
                fallbackTitle: nil
            )
        }

        if let title = extractedTitle(from: resolved) {
            resolved.debugTrail?.append("generic-title=\(title)")
            let matches = await searchService.searchDestinations(matching: title, limit: 1)
            if let destination = matches.first {
                return envelopeByResolving(resolved, with: destination)
            }
            resolved.debugTrail?.append("generic-title-search=no-match")
        }

        let remotePage = await ShareImportUtilities.remotePageSummary(for: resolvedURL)
        if let remotePage {
            resolved.originalURL = remotePage.finalURL.absoluteString
            resolved.debugTrail?.append("remote-page-url=\(remotePage.finalURL.absoluteString)")
        }

        if let remoteTitle = remotePage?.pageTitle {
            resolved.debugTrail?.append("remote-page-title=\(remoteTitle)")
            let matches = await searchService.searchDestinations(matching: remoteTitle, limit: 1)
            if let destination = matches.first {
                return envelopeByResolving(resolved, with: destination)
            }
            resolved.debugTrail?.append("remote-page-title-search=no-match")
        }

        if let remoteCoordinate = remotePage?.coordinate {
            resolved.debugTrail?.append("remote-page-coordinate=\(remoteCoordinate.latitude),\(remoteCoordinate.longitude)")
            return await resolvedEnvelopeForCoordinate(
                resolved,
                coordinate: remoteCoordinate,
                using: searchService,
                fallbackTitle: remotePage?.pageTitle
            )
        }

        resolved.disposition = .diagnosticsOnly
        resolved.note = resolved.classification == .googleMapsLocationLink
            ? "Google Maps link could not be resolved confidently yet."
            : "Shared location link did not expose a usable destination yet."
        return resolved
    }

    private func resolveGoogleMapsImport(
        _ envelope: SharedImportEnvelope,
        url: URL,
        using searchService: PlaceSearchService
    ) async -> SharedImportEnvelope {
        var resolved = envelope

        if let coordinate = ShareImportUtilities.extractCoordinate(from: url) {
            resolved.debugTrail?.append("google-coordinate=\(coordinate.latitude),\(coordinate.longitude)")
            return await resolvedEnvelopeForCoordinate(
                resolved,
                coordinate: coordinate,
                using: searchService,
                fallbackTitle: ShareImportUtilities.googleMapsQueryTitle(from: url) ?? extractedTitle(from: resolved)
            )
        }

        let queryTitle = ShareImportUtilities.googleMapsQueryTitle(from: url)
        let fallbackTitle = extractedTitle(from: resolved)
        let title = queryTitle ?? fallbackTitle
        if let title, !title.isEmpty {
            resolved.debugTrail?.append("google-title=\(title)")
            let matches = await searchService.searchDestinations(matching: title, limit: 1)
            if let destination = matches.first {
                return envelopeByResolving(resolved, with: destination)
            }
            resolved.debugTrail?.append("google-title-search=no-match")
        } else {
            resolved.debugTrail?.append("google-title=none")
        }

        resolved.disposition = .diagnosticsOnly
        resolved.note = "Google Maps link could not be resolved confidently yet."
        return resolved
    }

    private func envelopeByResolving(_ envelope: SharedImportEnvelope, with destination: DestinationSearchResult) -> SharedImportEnvelope {
        var resolved = envelope
        resolved.originalText = "\(destination.title)\n\(destination.coordinate.latitude),\(destination.coordinate.longitude)"
        resolved.disposition = .directHomePreview
        resolved.debugTrail = (resolved.debugTrail ?? []) + [
            "resolved-destination-title=\(destination.title)",
            "resolved-destination-coordinate=\(destination.coordinate.latitude),\(destination.coordinate.longitude)"
        ]
        return resolved
    }

    private func resolvedEnvelopeForCoordinate(
        _ envelope: SharedImportEnvelope,
        coordinate: CoordinatePoint,
        using searchService: PlaceSearchService,
        fallbackTitle: String?
    ) async -> SharedImportEnvelope {
        let fallback = fallbackTitle ?? sharedImportTitle(for: envelope)
        let resolvedDestination = await searchService.resolveDestination(
            at: coordinate,
            fallbackTitle: fallback,
            preserveFallbackTitle: shouldPreserveResolvedTitle(for: envelope, fallbackTitle: fallback)
        )
        let destination = resolvedDestination ?? DestinationSearchResult(
            id: envelope.id,
            title: fallback,
            subtitle: envelope.originalURL ?? envelope.originalText ?? "",
            coordinate: coordinate
        )
        return envelopeByResolving(envelope, with: destination)
    }

    private func shouldPreserveResolvedTitle(for envelope: SharedImportEnvelope, fallbackTitle: String) -> Bool {
        switch envelope.classification {
        case .googleMapsLocationLink:
            return isSpecificAddressLikeTitle(fallbackTitle)
        case .genericLocationLink, .gpxFile, .genericXMLFile, .fitFile, .tcxFile, .plainCoordinates, .unsupportedUnknown:
            return false
        }
    }

    private func isSpecificAddressLikeTitle(_ value: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return false }
        return trimmed.rangeOfCharacter(from: .decimalDigits) != nil
    }

    private func sharedFileProviderID(for envelope: SharedImportEnvelope) -> RouteProviderID? {
        switch envelope.classification {
        case .fitFile:
            return .fitImport
        case .tcxFile:
            return .tcxImport
        case .googleMapsLocationLink, .genericLocationLink, .gpxFile, .genericXMLFile, .plainCoordinates, .unsupportedUnknown:
            return nil
        }
    }

    private func sharedImportTitle(for envelope: SharedImportEnvelope) -> String {
        if let fileName = envelope.fileName?.trimmingCharacters(in: .whitespacesAndNewlines), !fileName.isEmpty {
            return URL(fileURLWithPath: fileName).deletingPathExtension().lastPathComponent
        }
        return extractedTitle(from: envelope)
            ?? (envelope.classification == .googleMapsLocationLink ? "Imported from Google Maps" : "Shared location")
    }

    private func sharedImportSourceLabel(for envelope: SharedImportEnvelope) -> String {
        switch envelope.classification {
        case .fitFile:
            return "FIT Import"
        case .tcxFile:
            return "TCX Import"
        case .googleMapsLocationLink:
            return "Google Maps"
        case .genericLocationLink, .gpxFile, .genericXMLFile, .plainCoordinates, .unsupportedUnknown:
            return "Shared"
        }
    }

    private func displayShareImportError(_ error: Error) -> String {
        if let localized = error as? LocalizedError, let message = localized.errorDescription {
            return message
        }
        return error.localizedDescription
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
           let name = queryItems.first(where: { ["q", "query", "destination", "daddr", "near", "name"].contains($0.name.lowercased()) })?.value,
           !name.isEmpty,
           ShareImportUtilities.extractCoordinate(from: name) == nil {
            return name.replacingOccurrences(of: "+", with: " ")
        }
        return nil
    }

    private func garminCourseFileEnvelope(from url: URL, envelope: SharedImportEnvelope) async -> (envelope: SharedImportEnvelope?, debugTrail: [String]) {
        guard let courseID = garminCourseID(from: url) else { return (nil, []) }

        var debugTrail = ["garmin-course-id=\(courseID)"]
        let candidateURLs = [
            "https://connect.garmin.com/proxy/activity-service-1.1/gpx/course/\(courseID)?full=true",
            "https://connect.garmin.com/proxy/course-service-1.0/gpx/course/\(courseID)"
        ].compactMap(URL.init(string:))

        for candidateURL in candidateURLs {
            debugTrail.append("garmin-course-try=\(candidateURL.absoluteString)")
            var request = URLRequest(url: candidateURL)
            request.timeoutInterval = 12
            request.cachePolicy = .reloadIgnoringLocalCacheData
            request.setValue("application/gpx+xml,application/xml,text/xml;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
            request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
            request.setValue(ShareImportUtilities.safariUserAgent, forHTTPHeaderField: "User-Agent")

            do {
                let (data, response) = try await URLSession.shared.data(for: request)
                guard let http = response as? HTTPURLResponse else {
                    debugTrail.append("garmin-course-response=non-http")
                    continue
                }
                debugTrail.append("garmin-course-status=\(http.statusCode)")
                guard (200 ..< 300).contains(http.statusCode) else {
                    continue
                }
                let xmlSnippet = ShareImportUtilities.decodeHTMLSnippet(data)
                guard xmlSnippet.localizedCaseInsensitiveContains("<gpx") else {
                    debugTrail.append("garmin-course-body=non-gpx")
                    continue
                }
                let fileName = garminCourseFileName(courseID: courseID, xml: xmlSnippet)
                let storedPath = try persistSharedImportData(data, fileName: fileName)
                var resolved = envelope
                resolved.classification = .gpxFile
                resolved.disposition = .directHomePreview
                resolved.fileName = fileName
                resolved.storedFilePath = storedPath
                resolved.note = "Imported Garmin course link as GPX."
                resolved.debugTrail = (resolved.debugTrail ?? []) + debugTrail + [
                    "garmin-course-gpx-url=\(candidateURL.absoluteString)",
                    "garmin-course-gpx-file=\(fileName)"
                ]
                return (resolved, debugTrail)
            } catch {
                debugTrail.append("garmin-course-error=\(error.localizedDescription)")
            }
        }

        if let jsonURL = URL(string: "https://connect.garmin.com/proxy/course-service-1.0/json/course/\(courseID)") {
            debugTrail.append("garmin-course-try=\(jsonURL.absoluteString)")
            var request = URLRequest(url: jsonURL)
            request.timeoutInterval = 12
            request.cachePolicy = .reloadIgnoringLocalCacheData
            request.setValue("application/json,text/plain;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
            request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
            request.setValue(ShareImportUtilities.safariUserAgent, forHTTPHeaderField: "User-Agent")

            do {
                let (data, response) = try await URLSession.shared.data(for: request)
                guard let http = response as? HTTPURLResponse else {
                    debugTrail.append("garmin-course-json-response=non-http")
                    return (nil, debugTrail)
                }
                debugTrail.append("garmin-course-json-status=\(http.statusCode)")
                if (200 ..< 300).contains(http.statusCode) {
                    let jsonAttempt = garminCourseEnvelopeFromJSON(
                        data: data,
                        courseID: courseID,
                        envelope: envelope,
                        debugTrail: debugTrail
                    )
                    debugTrail += jsonAttempt.debugTrail
                    if let resolved = jsonAttempt.envelope {
                        return (resolved, debugTrail)
                    }
                }
                debugTrail.append("garmin-course-json-body=unusable")
            } catch {
                debugTrail.append("garmin-course-json-error=\(error.localizedDescription)")
            }
        }

        return (nil, debugTrail)
    }

    private func garminCourseID(from url: URL) -> String? {
        guard let host = url.host?.lowercased(), host.contains("garmin.com") else { return nil }
        let components = url.pathComponents.filter { $0 != "/" }
        guard let courseIndex = components.firstIndex(of: "course"), courseIndex + 1 < components.count else {
            return nil
        }
        let candidate = components[courseIndex + 1]
        guard candidate.allSatisfy(\.isNumber) else { return nil }
        return candidate
    }

    private func garminCourseFileName(courseID: String, xml: String) -> String {
        if let title = ShareImportUtilities.firstMatch(in: xml, pattern: "<name[^>]*>(.*?)</name>")?
            .trimmingCharacters(in: .whitespacesAndNewlines),
           !title.isEmpty {
            let sanitized = title
                .replacingOccurrences(of: "/", with: "-")
                .replacingOccurrences(of: ":", with: "-")
                .replacingOccurrences(of: "\n", with: " ")
            return "\(sanitized).gpx"
        }
        return "garmin-course-\(courseID).gpx"
    }

    private func persistSharedImportData(_ data: Data, fileName: String) throws -> String {
        let directory = FileManager.default.temporaryDirectory.appendingPathComponent("SharedImports", isDirectory: true)
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let destination = directory.appendingPathComponent("\(UUID().uuidString)-\(fileName)")
        try data.write(to: destination, options: .atomic)
        return destination.path
    }

    private func garminCourseEnvelopeFromJSON(
        data: Data,
        courseID: String,
        envelope: SharedImportEnvelope,
        debugTrail: [String]
    ) -> (envelope: SharedImportEnvelope?, debugTrail: [String]) {
        let json = ShareImportUtilities.decodeHTMLSnippet(data)
        var extraDebug: [String] = [
            "garmin-course-json-bytes=\(data.count)"
        ]

        guard let object = try? JSONSerialization.jsonObject(with: data) else {
            extraDebug.append("garmin-course-json-parse=failed")
            extraDebug.append("garmin-course-json-preview=\(ShareImportUtilities.debugSnippet(for: json))")
            return (nil, extraDebug)
        }

        extraDebug.append("garmin-course-json-top=\(ShareImportUtilities.jsonShapeSummary(for: object))")

        let title = ShareImportUtilities.garminTitle(fromJSONObject: object)
            ?? ShareImportUtilities.garminCourseSummary(fromHTML: json)?.title
            ?? "garmin-course-\(courseID)"
        let coordinates = ShareImportUtilities.extractCoordinateSequence(fromGarminJSONObject: object)
        extraDebug.append("garmin-course-json-points=\(coordinates.count)")
        if coordinates.count < 2 {
            extraDebug.append("garmin-course-json-preview=\(ShareImportUtilities.debugSnippet(for: json))")
            return (nil, extraDebug)
        }

        let gpx = ShareImportUtilities.buildGPXDocument(name: title, coordinates: coordinates)
        guard let gpxData = gpx.data(using: .utf8),
              let storedPath = try? persistSharedImportData(gpxData, fileName: "\(ShareImportUtilities.sanitizeImportFileName(title)).gpx") else {
            extraDebug.append("garmin-course-json-gpx=persist-failed")
            return (nil, extraDebug)
        }

        var resolved = envelope
        resolved.classification = .gpxFile
        resolved.disposition = .directHomePreview
        resolved.fileName = "\(ShareImportUtilities.sanitizeImportFileName(title)).gpx"
        resolved.storedFilePath = storedPath
        resolved.note = "Imported Garmin course link via JSON fallback."
        resolved.debugTrail = (resolved.debugTrail ?? []) + debugTrail + extraDebug + [
            "garmin-course-json-file=\(resolved.fileName ?? "")"
        ]
        return (resolved, extraDebug)
    }

}
