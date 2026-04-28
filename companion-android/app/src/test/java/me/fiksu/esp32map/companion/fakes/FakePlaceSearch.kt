package me.fiksu.esp32map.companion.fakes

import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.DestinationSearchResult
import me.fiksu.esp32map.companion.integration.PlaceSearchService

class FakePlaceSearch : PlaceSearchService {
    var nextResults: List<DestinationSearchResult> = emptyList()
    var nextResolve: DestinationSearchResult? = null
    val searchCalls: MutableList<Pair<String, Int>> = mutableListOf()
    val resolveCalls: MutableList<CoordinatePoint> = mutableListOf()

    override suspend fun searchDestinations(
        query: String,
        limit: Int,
        riderBias: CoordinatePoint?,
    ): List<DestinationSearchResult> {
        searchCalls += query to limit
        return nextResults
    }

    override suspend fun resolveDestination(
        coordinate: CoordinatePoint,
        fallbackTitle: String,
    ): DestinationSearchResult? {
        resolveCalls += coordinate
        return nextResolve
    }
}
