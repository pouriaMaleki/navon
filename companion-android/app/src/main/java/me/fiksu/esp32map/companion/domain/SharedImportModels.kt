package me.fiksu.esp32map.companion.domain

data class SharedImportEnvelope(
    val id: String,
    val sourceApplication: String?,
    val receivedAtEpochMs: Long,
    val rawKind: SharedImportRawKind,
    val mimeType: String?,
    val uniformTypeIdentifier: String?,
    val fileName: String?,
    val fileSizeBytes: Int?,
    val originalText: String?,
    val originalUrl: String?,
    val storedFilePath: String?,
    val classification: SharedImportClassification,
    val disposition: SharedImportDisposition,
    val note: String?,
)

enum class SharedImportRawKind {
    URL,
    PLAIN_TEXT,
    FILE,
    MULTIPLE_ITEMS,
    UNKNOWN,
}

enum class SharedImportClassification {
    GOOGLE_MAPS_LOCATION_LINK,
    GENERIC_LOCATION_LINK,
    GPX_FILE,
    GENERIC_XML_FILE,
    FIT_FILE,
    TCX_FILE,
    PLAIN_COORDINATES,
    UNSUPPORTED_UNKNOWN,
}

enum class SharedImportDisposition {
    DIRECT_HOME_PREVIEW,
    ROUTE_DETAIL_REVIEW,
    DIAGNOSTICS_ONLY,
}

data class ImportDiagnosticsEntry(
    val id: String,
    val envelope: SharedImportEnvelope,
    val createdAtEpochMs: Long,
) {
    val title: String
        get() = envelope.fileName
            ?: envelope.originalUrl?.let { runCatching { java.net.URI(it).host }.getOrNull() }
            ?: envelope.sourceApplication
            ?: envelope.classification.name

    val subtitle: String
        get() = envelope.note
            ?: envelope.originalText?.trim()?.takeIf { it.isNotEmpty() }?.take(120)
            ?: envelope.originalUrl?.take(120)
            ?: "Unsupported shared item"
}
