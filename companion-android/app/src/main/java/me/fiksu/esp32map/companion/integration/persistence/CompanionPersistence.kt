package me.fiksu.esp32map.companion.integration.persistence

import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RouteSessionStore

class CompanionPersistence : RouteSessionStore {
    private val recentDestinations = mutableListOf<CoordinatePoint>()
    private var lastSession: ActiveRouteSession? = null
    private var settings: CompanionSettings = CompanionSettings()

    override fun loadRecentDestinations(): List<CoordinatePoint> = recentDestinations.toList()

    override fun saveRecentDestination(point: CoordinatePoint) {
        recentDestinations.add(0, point)
        while (recentDestinations.size > 10) {
            recentDestinations.removeLast()
        }
    }

    override fun loadLastSession(): ActiveRouteSession? = lastSession

    override fun saveSession(session: ActiveRouteSession) {
        lastSession = session
    }

    fun loadSettings(): CompanionSettings = settings

    fun saveSettings(newSettings: CompanionSettings) {
        settings = newSettings
    }
}
