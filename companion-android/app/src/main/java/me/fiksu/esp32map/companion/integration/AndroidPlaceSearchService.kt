package me.fiksu.esp32map.companion.integration

import android.content.Context
import android.location.Geocoder
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.DestinationSearchResult

interface PlaceSearchService {
    suspend fun searchDestinations(query: String, limit: Int): List<DestinationSearchResult>
    suspend fun resolveDestination(coordinate: CoordinatePoint, fallbackTitle: String = "Dropped pin"): DestinationSearchResult?
}

class AndroidPlaceSearchService(context: Context) : PlaceSearchService {
    private val geocoder = Geocoder(context, Locale.getDefault())

    override suspend fun searchDestinations(query: String, limit: Int): List<DestinationSearchResult> = withContext(Dispatchers.IO) {
        val trimmed = query.trim()
        if (trimmed.isEmpty()) return@withContext emptyList()
        runCatching {
            geocoder.getFromLocationName(trimmed, limit).orEmpty().mapIndexed { index, address ->
                DestinationSearchResult(
                    id = "search-$index-${address.latitude}-${address.longitude}",
                    title = address.featureName ?: address.thoroughfare ?: trimmed,
                    subtitle = listOfNotNull(address.locality, address.adminArea, address.countryName).joinToString(" • "),
                    coordinate = CoordinatePoint(address.latitude, address.longitude),
                )
            }
        }.getOrDefault(emptyList())
    }

    override suspend fun resolveDestination(coordinate: CoordinatePoint, fallbackTitle: String): DestinationSearchResult? = withContext(Dispatchers.IO) {
        runCatching {
            geocoder.getFromLocation(coordinate.latitude, coordinate.longitude, 1).orEmpty().firstOrNull()?.let { address ->
                DestinationSearchResult(
                    id = "reverse-${coordinate.latitude}-${coordinate.longitude}",
                    title = simpleTitle(address, fallbackTitle),
                    subtitle = listOfNotNull(address.locality, address.adminArea, address.countryName).joinToString(" • "),
                    coordinate = coordinate,
                )
            }
        }.getOrNull()
    }

    private fun simpleTitle(address: android.location.Address, fallbackTitle: String): String {
        val thoroughfare = address.thoroughfare?.takeIf { it.isNotBlank() }
        if (thoroughfare != null) {
            val parts = listOfNotNull(thoroughfare, address.subThoroughfare?.takeIf { it.isNotBlank() })
            if (parts.isNotEmpty()) {
                return parts.joinToString(" ")
            }
        }
        return address.featureName?.takeIf { it.isNotBlank() }
            ?: address.locality?.takeIf { it.isNotBlank() }
            ?: fallbackTitle
    }
}
