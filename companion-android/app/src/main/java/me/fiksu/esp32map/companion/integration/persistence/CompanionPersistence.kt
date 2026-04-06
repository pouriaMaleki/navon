package me.fiksu.esp32map.companion.integration.persistence

import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RoutePlannerPreferences
import me.fiksu.esp32map.companion.domain.RouteSessionStore

class CompanionPersistence : RouteSessionStore {
    private val recentDestinations = mutableListOf<CoordinatePoint>()
    private val routeHistory = mutableListOf<RouteHistoryItem>()
    private var lastSession: ActiveRouteSession? = null
    private var settings: CompanionSettings = CompanionSettings()
    private var plannerPreferences: RoutePlannerPreferences = RoutePlannerPreferences()

    override fun loadRecentDestinations(): List<CoordinatePoint> = recentDestinations.toList()

    override fun saveRecentDestination(point: CoordinatePoint) {
        recentDestinations.remove(point)
        recentDestinations.add(0, point)
        while (recentDestinations.size > 30) {
            recentDestinations.removeAt(recentDestinations.lastIndex)
        }
    }

    fun loadRecentRouteHistory(): List<RouteHistoryItem> = routeHistory.toList()

    fun saveRouteHistoryItem(item: RouteHistoryItem) {
        routeHistory.removeAll { it.id == item.id }
        routeHistory.add(0, item)
        while (routeHistory.size > 50) {
            routeHistory.removeAt(routeHistory.lastIndex)
        }
    }

    fun dismissRouteHistoryItem(id: String) {
        routeHistory.removeAll { it.id == id }
    }

    override fun loadLastSession(): ActiveRouteSession? = lastSession

    override fun saveSession(session: ActiveRouteSession) {
        lastSession = session
    }

    fun loadSettings(): CompanionSettings = settings

    fun saveSettings(newSettings: CompanionSettings) {
        settings = newSettings
    }

    fun loadRoutePlannerPreferences(): RoutePlannerPreferences = plannerPreferences

    fun saveRoutePlannerPreferences(preferences: RoutePlannerPreferences) {
        plannerPreferences = preferences
    }
}
