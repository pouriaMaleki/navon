package me.fiksu.esp32map.companion.integration.persistence

import android.content.Context
import com.google.gson.Gson
import com.google.gson.reflect.TypeToken
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionSettings
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.RouteHistoryItem
import me.fiksu.esp32map.companion.domain.RoutePlannerPreferences
import me.fiksu.esp32map.companion.domain.RouteSessionStore

class CompanionPersistence(context: Context? = null) : RouteSessionStore {
    private object Key {
        const val STORE = "companion.persistence"
        const val RECENT_DESTINATIONS = "recent_destinations"
        const val ROUTE_HISTORY = "route_history"
        const val LAST_SESSION = "last_session"
        const val SETTINGS = "settings"
        const val PLANNER_PREFERENCES = "planner_preferences"
    }

    private val defaults = context?.getSharedPreferences(Key.STORE, Context.MODE_PRIVATE)
    private val gson = Gson()

    private val recentDestinations = mutableListOf<CoordinatePoint>()
    private val routeHistory = mutableListOf<RouteHistoryItem>()
    private var lastSession: ActiveRouteSession? = null
    private var settings: CompanionSettings = CompanionSettings()
    private var plannerPreferences: RoutePlannerPreferences = RoutePlannerPreferences()

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
        items.remove(point)
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
        items.removeAll { it.id == item.id }
        items.add(0, item)
        while (items.size > 50) items.removeAt(items.lastIndex)
        if (defaults != null) {
            defaults.edit().putString(Key.ROUTE_HISTORY, gson.toJson(items)).apply()
        } else {
            routeHistory.clear()
            routeHistory.addAll(items)
        }
    }

    fun dismissRouteHistoryItem(id: String) {
        val items = loadRecentRouteHistory().toMutableList()
        items.removeAll { it.id == id }
        if (defaults != null) {
            defaults.edit().putString(Key.ROUTE_HISTORY, gson.toJson(items)).apply()
        } else {
            routeHistory.clear()
            routeHistory.addAll(items)
        }
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
            return gson.fromJson(stored, CompanionSettings::class.java)
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
}
