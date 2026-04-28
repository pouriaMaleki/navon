package me.fiksu.esp32map.companion.integration.persistence

import android.content.Context
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.sqrt
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.ImportDiagnosticsEntry
import me.fiksu.esp32map.companion.domain.PairedPeripheralRecord
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RouteHistorySource
import me.fiksu.esp32map.companion.domain.RoutePlannerPreferences
import me.fiksu.esp32map.companion.domain.RouteSessionStore
import me.fiksu.esp32map.companion.domain.SpeedUnit

class CompanionPersistence(context: Context? = null) : RouteSessionStore {
    private object Key {
        const val STORE = "companion.persistence"
        const val RECENT_DESTINATIONS = "recent_destinations"
        const val ROUTE_HISTORY = "route_history"
        const val IMPORT_DIAGNOSTICS = "import_diagnostics"
        const val LAST_SESSION = "last_session"
        const val SETTINGS = "settings"
        const val PLANNER_PREFERENCES = "planner_preferences"
        const val LAST_KNOWN_RIDER = "last_known_rider"
        const val PAIRED_PERIPHERAL = "paired_peripheral"
    }

    private val defaults = context?.getSharedPreferences(Key.STORE, Context.MODE_PRIVATE)
    private val gson = Gson()
    private val nearbyDestinationMergeThresholdMeters = 80.0

    private val recentDestinations = mutableListOf<CoordinatePoint>()
    private val routeHistory = mutableListOf<RouteHistoryItem>()
    private val importDiagnostics = mutableListOf<ImportDiagnosticsEntry>()
    private var lastSession: ActiveRouteSession? = null
    private var settings: CompanionSettings = CompanionSettings()
    private var plannerPreferences: RoutePlannerPreferences = RoutePlannerPreferences()
    private var pairedPeripheral: PairedPeripheralRecord? = null

    override fun loadRecentDestinations(): List<CoordinatePoint> {
        defaults?.let {
            val stored = it.getString(Key.RECENT_DESTINATIONS, null) ?: return recentDestinations.toList()
            val type = object : TypeToken<List<CoordinatePoint>>() {}.type
            return gson.fromJson(stored, type)
        }
        return recentDestinations.toList()
    }

    override fun saveRecentDestination(point: CoordinatePoint) {
        val items = loadRecentDestinations().toMutableList()
        items.removeAll { areNearby(it, point) }
        items.add(0, point)
        while (items.size > 30) items.removeAt(items.lastIndex)
        if (defaults != null) {
            defaults.edit().putString(Key.RECENT_DESTINATIONS, gson.toJson(items)).apply()
        } else {
            recentDestinations.clear()
            recentDestinations.addAll(items)
        }
    }

    fun loadRecentRouteHistory(): List<RouteHistoryItem> {
        defaults?.let {
            val stored = it.getString(Key.ROUTE_HISTORY, null) ?: return routeHistory.toList()
            val type = object : TypeToken<List<RouteHistoryItem>>() {}.type
            return gson.fromJson(stored, type)
        }
        return routeHistory.toList()
    }

    fun saveRouteHistoryItem(item: RouteHistoryItem) {
        val items = loadRecentRouteHistory().toMutableList()

        if (item.source == RouteHistorySource.RECENT_DESTINATION && item.destination != null) {
            val existingIndex = items.indexOfFirst { candidate ->
                candidate.source == RouteHistorySource.RECENT_DESTINATION &&
                    candidate.destination?.let { areNearby(it, item.destination) } == true
            }
            if (existingIndex >= 0) {
                val existing = items.removeAt(existingIndex)
                val merged = RouteHistoryItem(
                    id = existing.id,
                    title = preferredDestinationTitle(item.title, existing.title),
                    subtitle = item.subtitle,
                    source = RouteHistorySource.RECENT_DESTINATION,
                    sourceLabel = item.sourceLabel,
                    createdAtLabel = item.createdAtLabel,
                    destination = item.destination ?: existing.destination,
                    routePackage = null,
                    occurrenceCount = (existing.occurrenceCount ?: 1) + maxOf(item.occurrenceCount ?: 1, 1),
                )
                items.add(0, merged)
                persistRouteHistory(items)
                return
            }
        }

        items.removeAll { it.id == item.id }
        items.add(0, item)
        while (items.size > 50) items.removeAt(items.lastIndex)
        persistRouteHistory(items)
    }

    fun dismissRouteHistoryItem(id: String) {
        val items = loadRecentRouteHistory().toMutableList()
        items.removeAll { it.id == id }
        persistRouteHistory(items)
    }

    fun loadImportDiagnostics(): List<ImportDiagnosticsEntry> {
        defaults?.let {
            val stored = it.getString(Key.IMPORT_DIAGNOSTICS, null) ?: return importDiagnostics.toList()
            val type = object : TypeToken<List<ImportDiagnosticsEntry>>() {}.type
            return gson.fromJson(stored, type)
        }
        return importDiagnostics.toList()
    }

    fun saveImportDiagnosticsEntry(entry: ImportDiagnosticsEntry) {
        val items = loadImportDiagnostics().toMutableList()
        items.removeAll { it.id == entry.id }
        items.add(0, entry)
        while (items.size > 50) items.removeAt(items.lastIndex)
        persistImportDiagnostics(items)
    }

    fun dismissImportDiagnosticsEntry(id: String) {
        val items = loadImportDiagnostics().toMutableList()
        items.removeAll { it.id == id }
        persistImportDiagnostics(items)
    }

    override fun loadLastSession(): ActiveRouteSession? {
        defaults?.let {
            val stored = it.getString(Key.LAST_SESSION, null) ?: return lastSession
            return gson.fromJson(stored, ActiveRouteSession::class.java)
        }
        return lastSession
    }

    override fun saveSession(session: ActiveRouteSession) {
        if (defaults != null) {
            defaults.edit().putString(Key.LAST_SESSION, gson.toJson(session)).apply()
        } else {
            lastSession = session
        }
    }

    fun loadSettings(): CompanionSettings {
        defaults?.let {
            val stored = it.getString(Key.SETTINGS, null) ?: return settings
            // Gson uses reflection / Unsafe and bypasses Kotlin data-class
            // default values, so a stored blob written before the
            // `cyclingSpeedKph` / `speedUnit` fields existed would
            // deserialize them as 0.0 / null. Patch missing-or-invalid
            // values to the model defaults so old installs upgrade cleanly.
            val raw = gson.fromJson(stored, CompanionSettings::class.java) ?: return settings
            val fallback = CompanionSettings()
            // Gson reflection can leave `speedUnit` null on blobs written
            // before that field existed, even though the Kotlin type is
            // non-null. Cast to a nullable view to write the elvis safely
            // and silence the resulting "useless elvis" lint.
            @Suppress("USELESS_ELVIS")
            val resolvedSpeedUnit: SpeedUnit = (raw.speedUnit as SpeedUnit?) ?: fallback.speedUnit
            return raw.copy(
                cyclingSpeedKph = if (raw.cyclingSpeedKph > 0) raw.cyclingSpeedKph else fallback.cyclingSpeedKph,
                speedUnit = resolvedSpeedUnit,
            )
        }
        return settings
    }

    fun saveSettings(newSettings: CompanionSettings) {
        if (defaults != null) {
            defaults.edit().putString(Key.SETTINGS, gson.toJson(newSettings)).apply()
        } else {
            settings = newSettings
        }
    }

    fun loadPairedPeripheral(): PairedPeripheralRecord? {
        defaults?.let {
            val stored = it.getString(Key.PAIRED_PERIPHERAL, null) ?: return pairedPeripheral
            return runCatching {
                gson.fromJson(stored, PairedPeripheralRecord::class.java)
            }.getOrNull()
        }
        return pairedPeripheral
    }

    fun savePairedPeripheral(record: PairedPeripheralRecord) {
        if (defaults != null) {
            defaults.edit().putString(Key.PAIRED_PERIPHERAL, gson.toJson(record)).apply()
        } else {
            pairedPeripheral = record
        }
    }

    fun clearPairedPeripheral() {
        if (defaults != null) {
            defaults.edit().remove(Key.PAIRED_PERIPHERAL).apply()
        } else {
            pairedPeripheral = null
        }
    }

    fun loadRoutePlannerPreferences(): RoutePlannerPreferences {
        defaults?.let {
            val stored = it.getString(Key.PLANNER_PREFERENCES, null) ?: return plannerPreferences
            return gson.fromJson(stored, RoutePlannerPreferences::class.java)
        }
        return plannerPreferences
    }

    fun saveRoutePlannerPreferences(preferences: RoutePlannerPreferences) {
        if (defaults != null) {
            defaults.edit().putString(Key.PLANNER_PREFERENCES, gson.toJson(preferences)).apply()
        } else {
            plannerPreferences = preferences
        }
    }

    private fun persistRouteHistory(items: List<RouteHistoryItem>) {
        if (defaults != null) {
            defaults.edit().putString(Key.ROUTE_HISTORY, gson.toJson(items)).apply()
        } else {
            routeHistory.clear()
            routeHistory.addAll(items)
        }
    }

    private fun persistImportDiagnostics(items: List<ImportDiagnosticsEntry>) {
        if (defaults != null) {
            defaults.edit().putString(Key.IMPORT_DIAGNOSTICS, gson.toJson(items)).apply()
        } else {
            importDiagnostics.clear()
            importDiagnostics.addAll(items)
        }
    }

    private var lastKnownRider: CoordinatePoint? = null

    fun loadLastKnownRider(): CoordinatePoint? {
        defaults?.let {
            val stored = it.getString(Key.LAST_KNOWN_RIDER, null) ?: return lastKnownRider
            return runCatching { gson.fromJson(stored, CoordinatePoint::class.java) }.getOrNull()
        }
        return lastKnownRider
    }

    fun saveLastKnownRider(point: CoordinatePoint) {
        if (defaults != null) {
            defaults.edit().putString(Key.LAST_KNOWN_RIDER, gson.toJson(point)).apply()
        } else {
            lastKnownRider = point
        }
    }

    private fun preferredDestinationTitle(newTitle: String, existingTitle: String): String {
        return if (isGenericDestinationTitle(existingTitle) && !isGenericDestinationTitle(newTitle)) newTitle else existingTitle
    }

    private fun isGenericDestinationTitle(title: String): Boolean {
        val normalized = title.trim().lowercase()
        return normalized.isEmpty() || normalized == "dropped pin" || normalized == "recent destination" || normalized == "selected destination" || normalized == "route"
    }

    private fun areNearby(lhs: CoordinatePoint, rhs: CoordinatePoint): Boolean {
        return approximateDistanceMeters(lhs, rhs) <= nearbyDestinationMergeThresholdMeters
    }

    private fun approximateDistanceMeters(start: CoordinatePoint, end: CoordinatePoint): Double {
        val latMeters = (end.latitude - start.latitude) * 111_320.0
        val lonMeters = (end.longitude - start.longitude) * cos(((start.latitude + end.latitude) / 2.0) * PI / 180.0) * 111_320.0
        return sqrt(latMeters * latMeters + lonMeters * lonMeters)
    }
}
