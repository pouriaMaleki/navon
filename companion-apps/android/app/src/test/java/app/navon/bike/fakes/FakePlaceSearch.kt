package app.navon.bike.fakes

import app.navon.bike.domain.CoordinatePoint
import app.navon.bike.domain.DestinationSearchResult
import app.navon.bike.integration.PlaceSearchService

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
