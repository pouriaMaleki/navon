package app.navon.bike.integration.share

import android.content.ContentResolver
import android.content.Intent
import android.net.Uri
import android.provider.OpenableColumns
import android.annotation.SuppressLint
import java.util.UUID
import app.navon.bike.domain.SharedImportClassification
import app.navon.bike.domain.SharedImportDisposition
import app.navon.bike.domain.SharedImportEnvelope
import app.navon.bike.domain.SharedImportRawKind

class AndroidShareImportParser(private val contentResolver: ContentResolver) {
    fun parse(intent: Intent, sourceApplication: String?): List<SharedImportEnvelope> {
        val rawItems = rawItemsFrom(intent)
        if (rawItems.isEmpty()) return emptyList()
        return rawItems.mapIndexed { index, item ->
            when (item) {
                is RawSharedItem.Text -> classifyText(item.text, item.rawKind, sourceApplication, if (rawItems.size > 1 && index > 0) "Additional shared item ignored in v1." else null)
                is RawSharedItem.File -> classifyFile(item.uri, item.mimeType, item.rawKind, sourceApplication, if (rawItems.size > 1 && index > 0) "Additional shared item ignored in v1." else null)
            }
        }
    }

    @SuppressLint("NewApi")
    private fun rawItemsFrom(intent: Intent): List<RawSharedItem> {
        val action = intent.action.orEmpty()
        val multiple = mutableListOf<RawSharedItem>()
        when (action) {
            Intent.ACTION_SEND -> {
                intent.getStringExtra(Intent.EXTRA_TEXT)?.takeIf { it.isNotBlank() }?.let {
                    multiple += RawSharedItem.Text(it, SharedImportRawKind.PLAIN_TEXT)
                }
                (intent.getParcelableExtra(Intent.EXTRA_STREAM, Uri::class.java) ?: intent.data)?.let {
                    multiple += RawSharedItem.File(it, intent.type, SharedImportRawKind.FILE)
                }
            }
            Intent.ACTION_SEND_MULTIPLE -> {
                intent.getStringExtra(Intent.EXTRA_TEXT)?.takeIf { it.isNotBlank() }?.let {
                    multiple += RawSharedItem.Text(it, SharedImportRawKind.MULTIPLE_ITEMS)
                }
                val streams = intent.getParcelableArrayListExtra(Intent.EXTRA_STREAM, Uri::class.java).orEmpty()
                streams.forEach { uri ->
                    multiple += RawSharedItem.File(uri, intent.type, SharedImportRawKind.MULTIPLE_ITEMS)
                }
                val clipData = intent.clipData
                if (streams.isEmpty() && clipData != null) {
                    repeat(clipData.itemCount) { index ->
                        clipData.getItemAt(index).uri?.let { uri ->
                            multiple += RawSharedItem.File(uri, contentResolver.getType(uri), SharedImportRawKind.MULTIPLE_ITEMS)
                        }
                    }
                }
            }
            Intent.ACTION_VIEW -> {
                intent.data?.let { uri ->
                    if (uri.scheme == "content") {
                        multiple += RawSharedItem.File(uri, intent.type ?: contentResolver.getType(uri), SharedImportRawKind.URL)
                    } else {
                        multiple += RawSharedItem.Text(uri.toString(), SharedImportRawKind.URL)
                    }
                }
            }
        }
        return multiple
    }

    private fun classifyText(text: String, rawKind: SharedImportRawKind, sourceApplication: String?, note: String?): SharedImportEnvelope {
        val trimmed = text.trim()
        val normalizedUrl = trimmed.lineSequence().map(String::trim).firstOrNull { it.startsWith("http://") || it.startsWith("https://") }
        val titleLine = trimmed.lineSequence().map(String::trim).firstOrNull { it.isNotEmpty() && !it.startsWith("http://") && !it.startsWith("https://") }
        val url = normalizedUrl ?: trimmed.takeIf { it.startsWith("http://") || it.startsWith("https://") }
        val isGoogleMaps = url?.let(::isGoogleMapsUrl) == true
        val coordinateText = url ?: trimmed
        val hasCoordinates = extractCoordinate(coordinateText) != null
        val classification = when {
            isGoogleMaps -> SharedImportClassification.GOOGLE_MAPS_LOCATION_LINK
            hasCoordinates && url != null -> SharedImportClassification.GENERIC_LOCATION_LINK
            hasCoordinates -> SharedImportClassification.PLAIN_COORDINATES
            else -> SharedImportClassification.UNSUPPORTED_UNKNOWN
        }
        val disposition = when (classification) {
            SharedImportClassification.GOOGLE_MAPS_LOCATION_LINK,
            SharedImportClassification.GENERIC_LOCATION_LINK,
            SharedImportClassification.PLAIN_COORDINATES -> SharedImportDisposition.DIRECT_HOME_PREVIEW
            else -> SharedImportDisposition.DIAGNOSTICS_ONLY
        }
        return SharedImportEnvelope(
            id = UUID.randomUUID().toString(),
            sourceApplication = sourceApplication,
            receivedAtEpochMs = System.currentTimeMillis(),
            rawKind = rawKind,
            mimeType = "text/plain",
            uniformTypeIdentifier = null,
            fileName = null,
            fileSizeBytes = trimmed.toByteArray().size,
            originalText = if (titleLine != null && url != null) "$titleLine\n$url" else trimmed,
            originalUrl = url,
            storedFilePath = null,
            classification = classification,
            disposition = disposition,
            note = note,
        )
    }

    private fun classifyFile(uri: Uri, mimeType: String?, rawKind: SharedImportRawKind, sourceApplication: String?, note: String?): SharedImportEnvelope {
        val metadata = queryFileMetadata(uri)
        val fileName = metadata.first ?: uri.lastPathSegment?.substringAfterLast('/')
        val lowerName = fileName?.lowercase().orEmpty()
        val effectiveMime = mimeType ?: contentResolver.getType(uri)
        val classification = when {
            lowerName.endsWith(".gpx") || effectiveMime == "application/gpx+xml" -> SharedImportClassification.GPX_FILE
            lowerName.endsWith(".fit") -> SharedImportClassification.FIT_FILE
            lowerName.endsWith(".tcx") -> SharedImportClassification.TCX_FILE
            lowerName.endsWith(".xml") || effectiveMime == "application/xml" || effectiveMime == "text/xml" -> SharedImportClassification.GENERIC_XML_FILE
            else -> SharedImportClassification.UNSUPPORTED_UNKNOWN
        }
        val disposition = when (classification) {
            SharedImportClassification.GPX_FILE -> SharedImportDisposition.DIRECT_HOME_PREVIEW
            SharedImportClassification.GENERIC_XML_FILE,
            SharedImportClassification.FIT_FILE,
            SharedImportClassification.TCX_FILE,
            SharedImportClassification.UNSUPPORTED_UNKNOWN -> SharedImportDisposition.DIAGNOSTICS_ONLY
            else -> SharedImportDisposition.DIAGNOSTICS_ONLY
        }
        return SharedImportEnvelope(
            id = UUID.randomUUID().toString(),
            sourceApplication = sourceApplication,
            receivedAtEpochMs = System.currentTimeMillis(),
            rawKind = rawKind,
            mimeType = effectiveMime,
            uniformTypeIdentifier = null,
            fileName = fileName,
            fileSizeBytes = metadata.second,
            originalText = null,
            originalUrl = uri.toString(),
            storedFilePath = uri.toString(),
            classification = classification,
            disposition = disposition,
            note = note,
        )
    }

    private fun queryFileMetadata(uri: Uri): Pair<String?, Int?> {
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) {
                val name = cursor.getString(0)
                val size = if (cursor.isNull(1)) null else cursor.getLong(1).toInt()
                return name to size
            }
        }
        return null to null
    }

    private fun isGoogleMapsUrl(value: String): Boolean {
        val host = runCatching { Uri.parse(value).host?.lowercase().orEmpty() }.getOrDefault("")
        return host.contains("google.") || host == "maps.app.goo.gl" || host == "goo.gl"
    }

    private fun extractCoordinate(value: String): Pair<Double, Double>? {
        val pattern = Regex("""(-?\d{1,3}\.\d+)[,\s]+(-?\d{1,3}\.\d+)""")
        val match = pattern.find(value) ?: return null
        val latitude = match.groupValues[1].toDoubleOrNull() ?: return null
        val longitude = match.groupValues[2].toDoubleOrNull() ?: return null
        if (latitude !in -90.0..90.0 || longitude !in -180.0..180.0) return null
        return latitude to longitude
    }

    private sealed interface RawSharedItem {
        val rawKind: SharedImportRawKind

        data class Text(val text: String, override val rawKind: SharedImportRawKind) : RawSharedItem
        data class File(val uri: Uri, val mimeType: String?, override val rawKind: SharedImportRawKind) : RawSharedItem
    }
}
