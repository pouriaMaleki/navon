import Foundation

extension AppModel {
    private struct RemotePageSummary {
        let finalURL: URL
        let pageTitle: String?
        let coordinate: CoordinatePoint?
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
            origin: simulatedRiderLocation,
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
            .compactMap { extractCoordinate(from: $0) }
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
        if let urlString = extractURL(from: payload), let parsedURL = URL(string: urlString) {
            if let resolvedURLImport = await resolveURLImport(envelope: envelope, parsedURL: parsedURL, using: searchService) {
                return resolvedURLImport
            }
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
        let canonicalURL = canonicalSharedURL(parsedURL)
        let expandedURL = await expandedSharedURL(for: canonicalURL) ?? canonicalURL
        let resolvedURL = canonicalSharedURL(expandedURL)

        var resolved = envelope
        resolved.debugTrail = (resolved.debugTrail ?? []) + [
            "raw-url=\(parsedURL.absoluteString)",
            "canonical-url=\(canonicalURL.absoluteString)",
            "expanded-url=\(expandedURL.absoluteString)",
            "resolved-url=\(resolvedURL.absoluteString)"
        ]
        resolved.originalURL = resolvedURL.absoluteString
        if isGoogleMapsURL(resolved.originalURL ?? "") {
            resolved.classification = .googleMapsLocationLink
        } else if isSupportedSharedURL(resolvedURL) {
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

        if let coordinate = extractCoordinate(from: resolvedURL) {
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

        let remotePage = await remotePageSummary(for: resolvedURL)
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

        if let coordinate = extractCoordinate(from: url) {
            resolved.debugTrail?.append("google-coordinate=\(coordinate.latitude),\(coordinate.longitude)")
            return await resolvedEnvelopeForCoordinate(
                resolved,
                coordinate: coordinate,
                using: searchService,
                fallbackTitle: googleMapsQueryTitle(from: url) ?? extractedTitle(from: resolved)
            )
        }

        let queryTitle = googleMapsQueryTitle(from: url)
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
           extractCoordinate(from: name) == nil {
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
            request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")

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
                let xmlSnippet = decodeHTMLSnippet(data)
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
            request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")

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
        if let title = firstMatch(in: xml, pattern: "<name[^>]*>(.*?)</name>")?
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
        let json = decodeHTMLSnippet(data)
        var extraDebug: [String] = [
            "garmin-course-json-bytes=\(data.count)"
        ]

        guard let object = try? JSONSerialization.jsonObject(with: data) else {
            extraDebug.append("garmin-course-json-parse=failed")
            extraDebug.append("garmin-course-json-preview=\(debugSnippet(for: json))")
            return (nil, extraDebug)
        }

        extraDebug.append("garmin-course-json-top=\(jsonShapeSummary(for: object))")

        let title = garminTitle(fromJSONObject: object)
            ?? garminCourseSummary(fromHTML: json)?.title
            ?? "garmin-course-\(courseID)"
        let coordinates = extractCoordinateSequence(fromGarminJSONObject: object)
        extraDebug.append("garmin-course-json-points=\(coordinates.count)")
        if coordinates.count < 2 {
            extraDebug.append("garmin-course-json-preview=\(debugSnippet(for: json))")
            return (nil, extraDebug)
        }

        let gpx = buildGPXDocument(name: title, coordinates: coordinates)
        guard let gpxData = gpx.data(using: .utf8),
              let storedPath = try? persistSharedImportData(gpxData, fileName: "\(sanitizeImportFileName(title)).gpx") else {
            extraDebug.append("garmin-course-json-gpx=persist-failed")
            return (nil, extraDebug)
        }

        var resolved = envelope
        resolved.classification = .gpxFile
        resolved.disposition = .directHomePreview
        resolved.fileName = "\(sanitizeImportFileName(title)).gpx"
        resolved.storedFilePath = storedPath
        resolved.note = "Imported Garmin course link via JSON fallback."
        resolved.debugTrail = (resolved.debugTrail ?? []) + debugTrail + extraDebug + [
            "garmin-course-json-file=\(resolved.fileName ?? "")"
        ]
        return (resolved, extraDebug)
    }

    private func extractCoordinateSequence(fromGarminJSONObject object: Any) -> [CoordinatePoint] {
        var coordinates: [CoordinatePoint] = []
        collectCoordinateSequence(from: object, parentKey: nil, into: &coordinates)
        return coordinates
    }

    private func collectCoordinateSequence(from value: Any, parentKey: String?, into coordinates: inout [CoordinatePoint]) {
        if let dictionary = value as? [String: Any] {
            if let point = coordinate(fromJSONObjectDictionary: dictionary) {
                appendCoordinate(point, into: &coordinates)
            }

            for (key, nestedValue) in dictionary {
                if let string = nestedValue as? String,
                   let polylinePoints = decodedPolylineCoordinates(from: string, key: key),
                   polylinePoints.count >= 2 {
                    for point in polylinePoints {
                        appendCoordinate(point, into: &coordinates)
                    }
                    continue
                }
                collectCoordinateSequence(from: nestedValue, parentKey: key, into: &coordinates)
            }
            return
        }

        if let array = value as? [Any] {
            if let point = coordinate(fromJSONArray: array, parentKey: parentKey) {
                appendCoordinate(point, into: &coordinates)
                return
            }
            for nestedValue in array {
                collectCoordinateSequence(from: nestedValue, parentKey: parentKey, into: &coordinates)
            }
        }
    }

    private func coordinate(fromJSONObjectDictionary dictionary: [String: Any]) -> CoordinatePoint? {
        let latitudeKeys = ["latitude", "lat", "startlatitude", "startlat", "locationlatitude", "courselatitude"]
        let longitudeKeys = ["longitude", "lon", "lng", "startlongitude", "startlon", "startlng", "locationlongitude", "courselongitude"]

        let normalized = Dictionary(uniqueKeysWithValues: dictionary.map { ($0.key.lowercased(), $0.value) })
        guard let latitude = latitudeKeys.compactMap({ doubleValue(from: normalized[$0]) }).first,
              let longitude = longitudeKeys.compactMap({ doubleValue(from: normalized[$0]) }).first,
              (-90.0 ... 90.0).contains(latitude),
              (-180.0 ... 180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    private func coordinate(fromJSONArray array: [Any], parentKey: String?) -> CoordinatePoint? {
        guard array.count >= 2,
              let first = doubleValue(from: array[0]),
              let second = doubleValue(from: array[1]) else {
            return nil
        }

        let key = parentKey?.lowercased() ?? ""
        let preferGeoJSON = key.contains("coord") || key.contains("point") || key.contains("track") || key.contains("path")

        let lonLat = CoordinatePoint(latitude: second, longitude: first)
        if preferGeoJSON,
           (-90.0 ... 90.0).contains(lonLat.latitude),
           (-180.0 ... 180.0).contains(lonLat.longitude) {
            return lonLat
        }

        let latLon = CoordinatePoint(latitude: first, longitude: second)
        if (-90.0 ... 90.0).contains(latLon.latitude),
           (-180.0 ... 180.0).contains(latLon.longitude) {
            return latLon
        }

        if (-90.0 ... 90.0).contains(lonLat.latitude),
           (-180.0 ... 180.0).contains(lonLat.longitude) {
            return lonLat
        }

        return nil
    }

    private func decodedPolylineCoordinates(from value: String, key: String) -> [CoordinatePoint]? {
        let normalizedKey = key.lowercased()
        guard normalizedKey.contains("polyline") || normalizedKey.contains("encoded") || normalizedKey.contains("shape") else {
            return nil
        }
        let decoded = decodePolyline(value)
        return decoded.count >= 2 ? decoded : nil
    }

    private func decodePolyline(_ encoded: String) -> [CoordinatePoint] {
        guard !encoded.isEmpty else { return [] }
        var points: [CoordinatePoint] = []
        var index = encoded.startIndex
        var latitude = 0
        var longitude = 0

        func nextValue() -> Int? {
            var result = 0
            var shift = 0
            while index < encoded.endIndex {
                let value = Int(encoded[index].unicodeScalars.first!.value) - 63
                index = encoded.index(after: index)
                result |= (value & 0x1f) << shift
                shift += 5
                if value < 0x20 {
                    return (result & 1) == 0 ? (result >> 1) : ~(result >> 1)
                }
            }
            return nil
        }

        while let latDelta = nextValue(), let lonDelta = nextValue() {
            latitude += latDelta
            longitude += lonDelta
            points.append(
                CoordinatePoint(
                    latitude: Double(latitude) / 100_000.0,
                    longitude: Double(longitude) / 100_000.0
                )
            )
        }

        return points
    }

    private func appendCoordinate(_ point: CoordinatePoint, into coordinates: inout [CoordinatePoint]) {
        if coordinates.last != point {
            coordinates.append(point)
        }
    }

    private func doubleValue(from any: Any?) -> Double? {
        switch any {
        case let value as Double:
            return value
        case let value as NSNumber:
            return value.doubleValue
        case let value as String:
            return Double(value)
        default:
            return nil
        }
    }

    private func garminTitle(fromJSONObject object: Any) -> String? {
        if let dictionary = object as? [String: Any] {
            let preferredKeys = ["courseName", "displayName", "title", "name"]
            for key in preferredKeys {
                if let value = dictionary[key] as? String, !value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    return value
                }
            }
            for value in dictionary.values {
                if let title = garminTitle(fromJSONObject: value) {
                    return title
                }
            }
        } else if let array = object as? [Any] {
            for value in array {
                if let title = garminTitle(fromJSONObject: value) {
                    return title
                }
            }
        }
        return nil
    }

    private func jsonShapeSummary(for object: Any) -> String {
        if let dictionary = object as? [String: Any] {
            let keys = dictionary.keys.sorted().prefix(8).joined(separator: ",")
            return "dict[\(keys)]"
        }
        if let array = object as? [Any] {
            return "array[count=\(array.count)]"
        }
        return String(describing: type(of: object))
    }

    private func debugSnippet(for value: String) -> String {
        let collapsed = value.replacingOccurrences(of: "\\s+", with: " ", options: .regularExpression)
        return String(collapsed.prefix(240))
    }

    private func buildGPXDocument(name: String, coordinates: [CoordinatePoint]) -> String {
        let points = coordinates.map { point in
            String(format: "<trkpt lat=\"%.6f\" lon=\"%.6f\"></trkpt>", point.latitude, point.longitude)
        }.joined(separator: "\n")
        return """
        <?xml version="1.0" encoding="UTF-8"?>
        <gpx version="1.1" creator="ESP32MapCompanion" xmlns="http://www.topografix.com/GPX/1/1">
          <trk>
            <name>\(escapeXML(name))</name>
            <trkseg>
              \(points)
            </trkseg>
          </trk>
        </gpx>
        """
    }

    private func sanitizeImportFileName(_ value: String) -> String {
        value
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func escapeXML(_ value: String) -> String {
        value
            .replacingOccurrences(of: "&", with: "&amp;")
            .replacingOccurrences(of: "\"", with: "&quot;")
            .replacingOccurrences(of: "'", with: "&apos;")
            .replacingOccurrences(of: "<", with: "&lt;")
            .replacingOccurrences(of: ">", with: "&gt;")
    }

    private func googleMapsQueryTitle(from url: URL) -> String? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        let names = ["q", "query", "destination", "daddr"]
        for name in names {
            if let value = components.queryItems?.first(where: { $0.name.lowercased() == name })?.value {
                let normalized = value.replacingOccurrences(of: "+", with: " ").trimmingCharacters(in: .whitespacesAndNewlines)
                if !normalized.isEmpty, extractCoordinate(from: normalized) == nil {
                    return normalized
                }
            }
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

    private func isSupportedSharedURL(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else {
            return extractCoordinate(from: url) != nil
        }
        return isGoogleMapsURL(url.absoluteString)
            || host.contains("openstreetmap.org")
            || host.contains("garmin.com")
            || extractCoordinate(from: url) != nil
    }

    private func canonicalSharedURL(_ url: URL) -> URL {
        guard let host = url.host?.lowercased() else { return url }
        if host == "consent.google.com",
           let components = URLComponents(url: url, resolvingAgainstBaseURL: false),
           let continueValue = components.queryItems?.first(where: { $0.name == "continue" })?.value,
           let decoded = continueValue.removingPercentEncoding,
           let continueURL = URL(string: decoded) {
            return continueURL
        }
        if host == "connect.garmin.com",
           url.path.hasPrefix("/app/course/"),
           var components = URLComponents(url: url, resolvingAgainstBaseURL: false) {
            components.path = url.path.replacingOccurrences(of: "/app/course/", with: "/modern/course/")
            return components.url ?? url
        }
        return url
    }

    private func expandedSharedURL(for url: URL) async -> URL? {
        guard shouldExpandURL(url) else { return nil }
        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
        request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
        request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")
        do {
            let (_, response) = try await URLSession.shared.data(for: request)
            return response.url
        } catch {
            return nil
        }
    }

    private func shouldExpandURL(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host == "maps.app.goo.gl" || host == "goo.gl" || host == "consent.google.com"
    }

    private func remotePageSummary(for url: URL) async -> RemotePageSummary? {
        guard shouldInspectRemotePage(url) else { return nil }
        var request = URLRequest(url: url)
        request.timeoutInterval = 8
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.setValue("text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8", forHTTPHeaderField: "Accept")
        request.setValue("en-US,en;q=0.9", forHTTPHeaderField: "Accept-Language")
        request.setValue(safariUserAgent, forHTTPHeaderField: "User-Agent")

        do {
            let (data, response) = try await URLSession.shared.data(for: request)
            let finalURL = response.url ?? url
            let html = decodeHTMLSnippet(data)
            let garminFallback = garminCourseSummary(fromHTML: html)
            let pageTitle = sanitizedPageTitle(extractPageTitle(fromHTML: html) ?? garminFallback?.title)
            let coordinate = extractCoordinate(from: finalURL) ?? extractCoordinate(fromHTML: html) ?? garminFallback?.coordinate
            guard pageTitle != nil || coordinate != nil || finalURL.absoluteString != url.absoluteString else {
                return nil
            }
            return RemotePageSummary(finalURL: finalURL, pageTitle: pageTitle, coordinate: coordinate)
        } catch {
            return nil
        }
    }

    private func shouldInspectRemotePage(_ url: URL) -> Bool {
        guard let host = url.host?.lowercased() else { return false }
        return host.contains("garmin.com")
            || host.contains("openstreetmap.org")
    }

    private func decodeHTMLSnippet(_ data: Data) -> String {
        let snippet = Data(data.prefix(256_000))
        if let html = String(data: snippet, encoding: .utf8) {
            return html
        }
        if let html = String(data: snippet, encoding: .isoLatin1) {
            return html
        }
        return ""
    }

    private func extractPageTitle(fromHTML html: String) -> String? {
        if let ogTitle = firstMatch(in: html, pattern: "<meta[^>]+property=[\"']og:title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") {
            return ogTitle
        }
        if let twitterTitle = firstMatch(in: html, pattern: "<meta[^>]+name=[\"']twitter:title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") {
            return twitterTitle
        }
        if let metaTitle = firstMatch(in: html, pattern: "<meta[^>]+name=[\"']title[\"'][^>]+content=[\"']([^\"']+)[\"'][^>]*>") {
            return metaTitle
        }
        if let jsonLdTitle = firstMatch(in: html, pattern: "\"name\"\\s*:\\s*\"([^\"]+)\"") {
            return jsonLdTitle
        }
        return firstMatch(in: html, pattern: "<title[^>]*>(.*?)</title>")
    }

    private var safariUserAgent: String {
        "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Mobile/15E148 Safari/604.1"
    }

    private func garminCourseSummary(fromHTML html: String) -> (title: String?, coordinate: CoordinatePoint?)? {
        let title =
            firstMatch(in: html, pattern: "\"courseName\"\\s*:\\s*\"([^\"]+)\"")
            ?? firstMatch(in: html, pattern: "\"displayName\"\\s*:\\s*\"([^\"]+)\"")
            ?? firstMatch(in: html, pattern: "\"name\"\\s*:\\s*\"([^\"]+)\"")

        let coordinate =
            extractNamedCoordinatePair(
                from: html,
                latitudeNames: [
                    "startLatitude", "startLat", "start_location_lat", "courseLatitude",
                    "beginLatitude", "locationLatitude"
                ],
                longitudeNames: [
                    "startLongitude", "startLng", "startLon", "start_location_lng",
                    "courseLongitude", "beginLongitude", "locationLongitude"
                ]
            )
            ?? extractCoordinateFromCoordinateArray(in: html)

        guard title != nil || coordinate != nil else { return nil }
        return (title: title, coordinate: coordinate)
    }

    private func sanitizedPageTitle(_ rawTitle: String?) -> String? {
        guard let rawTitle else { return nil }
        var title = rawTitle
            .replacingOccurrences(of: "&amp;", with: "&")
            .replacingOccurrences(of: "&quot;", with: "\"")
            .replacingOccurrences(of: "&#39;", with: "'")
            .replacingOccurrences(of: "\n", with: " ")
            .trimmingCharacters(in: .whitespacesAndNewlines)

        for suffix in [" - Google Maps", " | Google Maps", " - OpenStreetMap", " | OpenStreetMap", " - Garmin Connect", " | Garmin Connect"] {
            if title.hasSuffix(suffix) {
                title.removeLast(suffix.count)
            }
        }

        if title.isEmpty || extractCoordinate(from: title) != nil {
            return nil
        }
        let lower = title.lowercased()
        if lower == "google maps" || lower == "openstreetmap" || lower == "garmin connect" {
            return nil
        }
        return title
    }

    private func extractCoordinate(from url: URL) -> CoordinatePoint? {
        guard let components = URLComponents(url: url, resolvingAgainstBaseURL: false) else { return nil }
        let host = url.host?.lowercased() ?? ""

        if let coordinate = extractCoordinate(from: components.queryItems) {
            return coordinate
        }
        if let fragment = components.fragment {
            if let coordinate = extractCoordinate(fromMapFragment: fragment) {
                return coordinate
            }
            if let coordinate = extractCoordinate(from: fragment) {
                return coordinate
            }
        }
        if host.contains("google."),
           let coordinate = extractCoordinate(fromGooglePath: url.path) {
            return coordinate
        }
        if !host.contains("google.") && host != "consent.google.com" {
            if let coordinate = extractCoordinate(from: url.absoluteString.removingPercentEncoding ?? url.absoluteString) {
                return coordinate
            }
        }
        return nil
    }

    private func extractCoordinate(fromGooglePath path: String) -> CoordinatePoint? {
        guard let groups = firstMatchGroups(in: path, pattern: "@(-?\\d{1,3}\\.\\d+),(-?\\d{1,3}\\.\\d+)"),
              groups.count == 2,
              let latitude = Double(groups[0]),
              let longitude = Double(groups[1]),
              (-90.0 ... 90.0).contains(latitude),
              (-180.0 ... 180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    private func extractCoordinate(from queryItems: [URLQueryItem]?) -> CoordinatePoint? {
        guard let queryItems else { return nil }

        var namedValues: [String: String] = [:]
        for item in queryItems where namedValues[item.name.lowercased()] == nil {
            namedValues[item.name.lowercased()] = item.value ?? ""
        }
        let latitudeKeys = ["lat", "latitude", "mlat"]
        let longitudeKeys = ["lon", "lng", "longitude", "mlon"]
        if let latitudeString = latitudeKeys.compactMap({ namedValues[$0] }).first(where: { !$0.isEmpty }),
           let longitudeString = longitudeKeys.compactMap({ namedValues[$0] }).first(where: { !$0.isEmpty }),
           let latitude = Double(latitudeString),
           let longitude = Double(longitudeString),
           (-90.0 ... 90.0).contains(latitude),
           (-180.0 ... 180.0).contains(longitude) {
            return CoordinatePoint(latitude: latitude, longitude: longitude)
        }

        for name in ["ll", "sll", "center", "destination", "daddr", "near", "q", "query"] {
            if let value = namedValues[name]?.replacingOccurrences(of: "loc:", with: ""),
               let coordinate = extractCoordinate(from: value) {
                return coordinate
            }
        }

        return nil
    }

    private func extractCoordinate(fromMapFragment fragment: String) -> CoordinatePoint? {
        guard let zoomlessMatch = firstMatchGroups(in: fragment, pattern: "map=\\d+(?:\\.\\d+)?/(-?\\d{1,3}\\.\\d+)/(-?\\d{1,3}\\.\\d+)"),
              zoomlessMatch.count == 2,
              let latitude = Double(zoomlessMatch[0]),
              let longitude = Double(zoomlessMatch[1]),
              (-90.0 ... 90.0).contains(latitude),
              (-180.0 ... 180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    private func extractCoordinate(fromHTML html: String) -> CoordinatePoint? {
        if let named = extractNamedCoordinatePair(
            from: html,
            latitudeNames: ["latitude", "lat", "startLatitude", "endLatitude", "centerLatitude"],
            longitudeNames: ["longitude", "lng", "lon", "startLongitude", "endLongitude", "centerLongitude"]
        ) {
            return named
        }
        return extractCoordinate(from: html)
    }

    private func extractCoordinateFromCoordinateArray(in value: String) -> CoordinatePoint? {
        guard let groups = firstMatchGroups(
            in: value,
            pattern: "\"coordinates\"\\s*:\\s*\\[\\s*(-?\\d{1,3}\\.\\d+)\\s*,\\s*(-?\\d{1,3}\\.\\d+)\\s*\\]"
        ),
        groups.count == 2,
        let longitude = Double(groups[0]),
        let latitude = Double(groups[1]),
        (-90.0 ... 90.0).contains(latitude),
        (-180.0 ... 180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    private func extractNamedCoordinatePair(
        from value: String,
        latitudeNames: [String],
        longitudeNames: [String]
    ) -> CoordinatePoint? {
        for latitudeName in latitudeNames {
            for longitudeName in longitudeNames {
                let pattern = "\(latitudeName)[\"'=:\\s>]+(-?\\d{1,3}\\.\\d+).*?\(longitudeName)[\"'=:\\s>]+(-?\\d{1,3}\\.\\d+)"
                if let groups = firstMatchGroups(in: value, pattern: pattern),
                   groups.count == 2,
                   let latitude = Double(groups[0]),
                   let longitude = Double(groups[1]),
                   (-90.0 ... 90.0).contains(latitude),
                   (-180.0 ... 180.0).contains(longitude) {
                    return CoordinatePoint(latitude: latitude, longitude: longitude)
                }
            }
        }
        return nil
    }

    private func extractCoordinate(from value: String) -> CoordinatePoint? {
        let pattern = try? NSRegularExpression(pattern: "(-?\\d{1,3}\\.\\d+)[,\\s/]+(-?\\d{1,3}\\.\\d+)")
        guard let match = pattern?.firstMatch(in: value, range: NSRange(location: 0, length: value.utf16.count)),
              let latitudeRange = Range(match.range(at: 1), in: value),
              let longitudeRange = Range(match.range(at: 2), in: value),
              let latitude = Double(value[latitudeRange]),
              let longitude = Double(value[longitudeRange]),
              (-90.0 ... 90.0).contains(latitude),
              (-180.0 ... 180.0).contains(longitude) else {
            return nil
        }
        return CoordinatePoint(latitude: latitude, longitude: longitude)
    }

    private func firstMatch(in value: String, pattern: String) -> String? {
        guard let groups = firstMatchGroups(in: value, pattern: pattern), let first = groups.first else {
            return nil
        }
        return first
    }

    private func firstMatchGroups(in value: String, pattern: String) -> [String]? {
        guard let regex = try? NSRegularExpression(pattern: pattern, options: [.caseInsensitive, .dotMatchesLineSeparators]) else {
            return nil
        }
        let fullRange = NSRange(location: 0, length: value.utf16.count)
        guard let match = regex.firstMatch(in: value, range: fullRange), match.numberOfRanges > 1 else {
            return nil
        }
        return (1 ..< match.numberOfRanges).compactMap { index in
            guard let range = Range(match.range(at: index), in: value) else { return nil }
            return String(value[range]).trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }
}
