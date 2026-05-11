package app.navon.bike.integration.cycling

import java.net.HttpURLConnection
import java.net.URL
import java.net.URLEncoder
import java.util.Locale
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import app.navon.bike.domain.CoordinatePoint
import org.json.JSONObject

/**
 * BRouter is a free, public, OSM-based cycling routing service. Each call
 * returns ONE route. The orchestrator calls multiple profiles in parallel
 * to surface alternatives with different cycle-infrastructure trade-offs.
 *
 * `timode=2` is required to populate `voicehints`; without it BRouter
 * returns a route with no turn instructions.
 */
enum class BrouterProfile(val key: String) {
    FASTBIKE("fastbike"),
    TREKKING("trekking"),
    SAFETY("safety"),
}

object BrouterClient {
    private const val BASE = "https://brouter.de/brouter"

    suspend fun fetch(
        profile: BrouterProfile,
        origin: CoordinatePoint,
        destination: CoordinatePoint,
    ): JSONObject = withContext(Dispatchers.IO) {
        val lonlats = String.format(
            Locale.US,
            "%.6f,%.6f|%.6f,%.6f",
            origin.longitude,
            origin.latitude,
            destination.longitude,
            destination.latitude,
        )
        val url = URL(
            "$BASE?lonlats=${URLEncoder.encode(lonlats, "UTF-8")}" +
                "&profile=${profile.key}&alternativeidx=0&format=geojson&timode=2",
        )
        val connection = (url.openConnection() as HttpURLConnection).apply {
            requestMethod = "GET"
            connectTimeout = 10_000
            readTimeout = 10_000
        }
        val statusCode = connection.responseCode
        val body = runCatching {
            val stream = if (statusCode in 200..299) connection.inputStream else connection.errorStream
            stream?.bufferedReader()?.use { it.readText() }.orEmpty()
        }.getOrDefault("")
        if (statusCode !in 200..299) {
            throw IllegalStateException("HTTP $statusCode: $body")
        }
        val root = JSONObject(body)
        val features = root.optJSONArray("features")
        if (features == null || features.length() == 0) {
            throw IllegalStateException("BRouter returned no features")
        }
        features.getJSONObject(0)
    }
}
