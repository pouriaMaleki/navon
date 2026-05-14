package app.navon.bike.integration.diagnostics

import app.navon.bike.domain.*
import app.navon.bike.integration.persistence.CompanionPersistence
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.flow.update

class RoutingDiagnosticsStore(private val persistence: CompanionPersistence) {

    private val _sessions = MutableStateFlow<List<RoutingDiagSession>>(emptyList())
    val sessions: StateFlow<List<RoutingDiagSession>> = _sessions.asStateFlow()

    private val _currentSession = MutableStateFlow<RoutingDiagSession?>(null)
    val currentSession: StateFlow<RoutingDiagSession?> = _currentSession.asStateFlow()

    val isRecording: Boolean get() = _currentSession.value != null

    init {
        _sessions.value = persistence.loadRoutingDiagnosticsSessions()
    }

    fun startRecording() {
        if (isRecording) return
        _currentSession.value = RoutingDiagSession(
            id = newSessionId(),
            createdAtMs = nowMs(),
        )
    }

    fun stopRecording() {
        val session = _currentSession.value ?: return
        val finalized = session.copy(
            updatedAtMs = nowMs(),
            events = session.events.toList(),
        )
        _currentSession.value = null
        persistence.saveRoutingDiagnosticsSession(finalized)
        _sessions.value = persistence.loadRoutingDiagnosticsSessions()
    }

    fun recordEvent(data: RoutingDiagEventData) {
        _currentSession.update { session ->
            if (session == null) return
            val event = RoutingDiagEvent(
                id = newEventId(),
                timestampMs = nowMs(),
                data = data,
            )
            session.copy(
                updatedAtMs = nowMs(),
                events = session.events + event,
            )
        }
    }

    fun recordRouteGeometry(routeId: String, providerName: String, geometry: List<CoordinatePoint>) {
        _currentSession.update { session ->
            if (session == null) return
            val existing = session.routeGeometries ?: emptyList()
            if (existing.any { it.routeId == routeId }) return
            val entry = RouteGeometryEntry(
                routeId = routeId,
                providerName = providerName,
                geometry = geometry,
            )
            session.copy(routeGeometries = existing + entry)
        }
    }

    fun deleteSession(id: String) {
        persistence.dismissRoutingDiagnosticsSession(id)
        _sessions.value = persistence.loadRoutingDiagnosticsSessions()
    }

    fun debugPackageText(id: String): String? {
        return _sessions.value.find { it.id == id }?.debugPackageText()
    }
}
