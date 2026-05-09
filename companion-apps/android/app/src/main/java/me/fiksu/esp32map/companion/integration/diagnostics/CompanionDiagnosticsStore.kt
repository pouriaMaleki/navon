package me.fiksu.esp32map.companion.integration.diagnostics

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import me.fiksu.esp32map.companion.domain.ActiveRouteSession
import me.fiksu.esp32map.companion.domain.CompanionDiagnostics
import me.fiksu.esp32map.companion.domain.SyncSessionState

class CompanionDiagnosticsStore {
    private val mutableState = MutableStateFlow(
        CompanionDiagnostics(
            providerName = "HSL",
            routeIdentifier = "none",
            routeRevision = 0,
            bleState = "DISCONNECTED",
            lastSyncResult = "Not sent yet",
            lastRerouteOutcome = "No reroute yet",
        ),
    )

    val state: StateFlow<CompanionDiagnostics> = mutableState.asStateFlow()

    fun update(session: ActiveRouteSession?, syncState: SyncSessionState) {
        mutableState.value = CompanionDiagnostics(
            providerName = session?.providerId?.displayName ?: "HSL",
            routeIdentifier = session?.routeIdentifier ?: "none",
            routeRevision = session?.routeRevision ?: 0,
            bleState = syncState.connectionState.name,
            lastSyncResult = syncState.lastSyncResult,
            lastRerouteOutcome = session?.lastRerouteReason ?: "No reroute yet",
        )
    }
}
