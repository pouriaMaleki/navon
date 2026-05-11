package app.navon.bike.integration.diagnostics

import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import app.navon.bike.domain.ActiveRouteSession
import app.navon.bike.domain.CompanionDiagnostics
import app.navon.bike.domain.SyncSessionState

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
