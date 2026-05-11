package app.navon.bike.app

import android.app.Application
import android.content.Intent
import android.net.Uri
import java.util.UUID
import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.ImportDiagnosticsEntry
import app.navon.bike.domain.RouteHistoryItem
import app.navon.bike.domain.RouteHistorySource
import app.navon.bike.domain.RoutePlanRequest
import app.navon.bike.domain.SharedImportClassification
import app.navon.bike.domain.SharedImportDisposition
import app.navon.bike.domain.SharedImportEnvelope
import app.navon.bike.integration.share.AndroidShareImportParser

private const val SHARED_INTENT_CONSUMED_EXTRA = "app.navon.bike.SHARED_INTENT_CONSUMED"

internal fun shareImportHandleIntent(appState: CompanionAppState, intent: Intent?, sourceApplication: String?): Boolean {
    if (!shouldHandleSharedIntent(intent)) return false
    intent ?: return false
    val envelopes = AndroidShareImportParser(appState.getApplication<Application>().contentResolver).parse(intent, sourceApplication)
    if (envelopes.isEmpty()) return true
    val primary = envelopes.firstOrNull { it.disposition == SharedImportDisposition.DIRECT_HOME_PREVIEW }
    envelopes.filter { it.id != primary?.id }.forEach { saveImportDiagnostic(appState, it) }
    if (primary != null) {
        handleSharedImportEnvelope(appState, primary)
    } else {
        saveImportDiagnostic(appState, envelopes.first())
        appState.shareImportEventId = System.currentTimeMillis()
    }
    return true
}

internal fun shareImportRetry(appState: CompanionAppState, entry: ImportDiagnosticsEntry) {
    handleSharedImportEnvelope(appState, entry.envelope.copy(id = UUID.randomUUID().toString()))
    appState.dismissImportDiagnosticsEntry(entry.id)
}

private fun handleSharedImportEnvelope(appState: CompanionAppState, envelope: SharedImportEnvelope) {
    when (envelope.classification) {
        SharedImportClassification.GPX_FILE -> {
            val uri = envelope.storedFilePath?.let(Uri::parse) ?: return
            appState.importGpxUri(appState.getApplication<Application>().applicationContext, uri)
            appState.shareImportEventId = System.currentTimeMillis()
        }
        SharedImportClassification.GOOGLE_MAPS_LOCATION_LINK,
        SharedImportClassification.GENERIC_LOCATION_LINK,
        SharedImportClassification.PLAIN_COORDINATES -> {
            val coordinate = extractCoordinate(envelope.originalUrl ?: envelope.originalText.orEmpty()) ?: run {
                saveImportDiagnostic(appState, envelope.copy(disposition = SharedImportDisposition.DIAGNOSTICS_ONLY, note = "Shared location could not be resolved."))
                return
            }
            val title = extractSharedTitle(envelope)
                ?: if (envelope.classification == SharedImportClassification.GOOGLE_MAPS_LOCATION_LINK) "Imported from Google Maps" else "Shared location"
            appState.routeRequest = RoutePlanRequest(
                origin = appState.riderLocation,
                destination = coordinate,
                providerId = appState.currentSourceMode.primaryProviderId,
            )
            appState.recordRecentDestination(title, coordinate)
            appState.planRoute(appState.currentSourceMode, preferredTitle = title) {
                val source = if (envelope.classification == SharedImportClassification.GOOGLE_MAPS_LOCATION_LINK) RouteHistorySource.GOOGLE_MAPS else RouteHistorySource.SHARE_IMPORT
                val sourceLabel = if (source == RouteHistorySource.GOOGLE_MAPS) "Google Maps" else "Shared"
                recordImportedPreview(appState, title, source, sourceLabel)
                appState.shareImportEventId = System.currentTimeMillis()
            }
        }
        else -> saveImportDiagnostic(appState, envelope)
    }
}

private fun recordImportedPreview(appState: CompanionAppState, title: String, source: RouteHistorySource, sourceLabel: String) {
    val selected = appState.preview.selectedAlternative?.normalizedPackage ?: return
    appState.persistence.saveRouteHistoryItem(
        RouteHistoryItem(
            id = selected.routeIdentifier,
            title = title,
            subtitle = selected.summaryLine,
            source = source,
            sourceLabel = sourceLabel,
            createdAtLabel = "Just now",
            destination = selected.geometry.lastOrNull(),
            routePackage = selected,
            occurrenceCount = null,
        ),
    )
    appState.notePersistenceChanged()
}

private fun saveImportDiagnostic(appState: CompanionAppState, envelope: SharedImportEnvelope) {
    appState.persistence.saveImportDiagnosticsEntry(
        ImportDiagnosticsEntry(
            id = envelope.id,
            envelope = envelope,
            createdAtEpochMs = System.currentTimeMillis(),
        ),
    )
    appState.notePersistenceChanged()
}

private fun extractSharedTitle(envelope: SharedImportEnvelope): String? {
    val text = envelope.originalText ?: envelope.originalUrl.orEmpty()
    val lines = text.lines().map { it.trim() }.filter { it.isNotEmpty() }
    lines.firstOrNull { !it.startsWith("http://") && !it.startsWith("https://") }?.let { return it }
    val url = envelope.originalUrl ?: return null
    val parsed = Uri.parse(url)
    val query = parsed.getQueryParameter("q") ?: parsed.getQueryParameter("query") ?: parsed.getQueryParameter("destination")
    return query?.replace('+', ' ')?.takeIf { extractCoordinate(it) == null }
}

private fun extractCoordinate(value: String): CoordinatePoint? {
    val match = Regex("""(-?\d{1,3}\.\d+)[,\s]+(-?\d{1,3}\.\d+)""").find(value) ?: return null
    val latitude = match.groupValues[1].toDoubleOrNull() ?: return null
    val longitude = match.groupValues[2].toDoubleOrNull() ?: return null
    if (latitude !in -90.0..90.0 || longitude !in -180.0..180.0) return null
    return CoordinatePoint(latitude, longitude)
}

internal fun shouldHandleSharedIntent(intent: Intent?): Boolean {
    intent ?: return false
    if (intent.getBooleanExtra(SHARED_INTENT_CONSUMED_EXTRA, false)) return false
    return when (intent.action) {
        Intent.ACTION_SEND,
        Intent.ACTION_SEND_MULTIPLE,
        Intent.ACTION_VIEW -> true
        else -> false
    }
}

internal fun markSharedIntentConsumed(intent: Intent) {
    intent.putExtra(SHARED_INTENT_CONSUMED_EXTRA, true)
}
