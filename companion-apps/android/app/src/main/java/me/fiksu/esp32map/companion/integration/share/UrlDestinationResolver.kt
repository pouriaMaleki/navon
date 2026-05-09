package me.fiksu.esp32map.companion.integration.share

import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.withContext
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import java.net.HttpURLConnection
import java.net.URL

/**
 * Result of asking the share-import classifier to follow a URL to a destination.
 * Mirrors the iOS `UrlDestinationResolution` enum and the web `resolveUrlDestination` flow.
 */
sealed interface UrlDestinationResolution {
    data class Coordinate(val point: CoordinatePoint, val suggestedTitle: String?) : UrlDestinationResolution
    data object NoDestinationFound : UrlDestinationResolution
    data class NetworkError(val message: String) : UrlDestinationResolution
}

private val COORD_AT = Regex("""@(-?\d{1,3}(?:\.\d+)?),(-?\d{1,3}(?:\.\d+)?)""")
private val COORD_QUERY = Regex("""[?&]q=(-?\d{1,3}(?:\.\d+)?),(-?\d{1,3}(?:\.\d+)?)""")
private val DAATA_LL = Regex("""!3d(-?\d{1,3}(?:\.\d+)?)!4d(-?\d{1,3}(?:\.\d+)?)""")
private val PROPERTY_GEO = Regex(
    """"geo":\s*\{[^}]*?"lat(?:itude)?":\s*(-?\d+\.?\d*)[^}]*?"l(?:on|ng)(?:itude)?":\s*(-?\d+\.?\d*)""",
)
private val TITLE_TAG = Regex("""<title>([^<]*)</title>""", RegexOption.IGNORE_CASE)
private const val USER_AGENT = "esp32-map-companion-android/0.1"
private const val MAX_BODY_BYTES = 256 * 1024

/** Try to extract a coordinate from a URL string directly (no network). */
fun extractCoordinateFromText(text: String): CoordinatePoint? {
    return matchCoord(COORD_AT, text)
        ?: matchCoord(COORD_QUERY, text)
        ?: matchCoord(DAATA_LL, text)
        ?: matchCoord(PROPERTY_GEO, text)
}

/** Follow the URL with HttpURLConnection (handles redirects natively). */
suspend fun resolveDestinationFromUrl(urlString: String): UrlDestinationResolution = withContext(Dispatchers.IO) {
    val trimmed = urlString.trim()
    if (trimmed.isEmpty()) return@withContext UrlDestinationResolution.NoDestinationFound

    extractCoordinateFromText(trimmed)?.let {
        return@withContext UrlDestinationResolution.Coordinate(it, suggestedTitle = null)
    }

    val parsed = runCatching { URL(trimmed) }.getOrNull()
        ?: return@withContext UrlDestinationResolution.NoDestinationFound

    try {
        val connection = (parsed.openConnection() as HttpURLConnection).apply {
            instanceFollowRedirects = true
            connectTimeout = 10_000
            readTimeout = 10_000
            setRequestProperty("User-Agent", USER_AGENT)
            setRequestProperty("Accept", "text/html,application/xhtml+xml,application/json;q=0.9,*/*;q=0.8")
        }
        try {
            val status = connection.responseCode
            val finalUrl = connection.url.toString()
            extractCoordinateFromText(finalUrl)?.let {
                return@withContext UrlDestinationResolution.Coordinate(it, suggestedTitle = null)
            }
            if (status !in 200..399) {
                return@withContext UrlDestinationResolution.NetworkError("HTTP $status")
            }
            val body = connection.inputStream.use { stream ->
                stream.bufferedReader().use { reader ->
                    val builder = StringBuilder()
                    val chunk = CharArray(8 * 1024)
                    var total = 0
                    while (true) {
                        val read = reader.read(chunk)
                        if (read <= 0) break
                        builder.append(chunk, 0, read)
                        total += read
                        if (total >= MAX_BODY_BYTES) break
                    }
                    builder.toString()
                }
            }
            val coordinate = extractCoordinateFromText(body)
            val title = TITLE_TAG.find(body)?.groupValues?.getOrNull(1)?.trim()?.takeIf { it.isNotBlank() }
            if (coordinate != null) {
                return@withContext UrlDestinationResolution.Coordinate(coordinate, title)
            }
            return@withContext UrlDestinationResolution.NoDestinationFound
        } finally {
            connection.disconnect()
        }
    } catch (err: Exception) {
        UrlDestinationResolution.NetworkError(err.message ?: err::class.java.simpleName)
    }
}

private fun matchCoord(regex: Regex, source: String): CoordinatePoint? {
    val match = regex.find(source) ?: return null
    val lat = match.groupValues.getOrNull(1)?.toDoubleOrNull() ?: return null
    val lon = match.groupValues.getOrNull(2)?.toDoubleOrNull() ?: return null
    if (lat !in -90.0..90.0 || lon !in -180.0..180.0) return null
    return CoordinatePoint(latitude = lat, longitude = lon)
}
