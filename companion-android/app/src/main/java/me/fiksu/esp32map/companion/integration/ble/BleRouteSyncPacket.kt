package me.fiksu.esp32map.companion.integration.ble

import kotlin.math.ceil
import me.fiksu.esp32map.companion.domain.CoordinatePoint
import me.fiksu.esp32map.companion.domain.NormalizedRoutePackage
import me.fiksu.esp32map.companion.domain.RouteClearMessage
import me.fiksu.esp32map.companion.domain.RouteManeuver
import me.fiksu.esp32map.companion.domain.RouteManeuverType
import me.fiksu.esp32map.companion.domain.RoutePackageVersion
import me.fiksu.esp32map.companion.domain.RouteProvenance
import me.fiksu.esp32map.companion.domain.RouteProviderId
import me.fiksu.esp32map.companion.domain.RouteRerouteRequestMessage
import me.fiksu.esp32map.companion.domain.RouteSetMessage
import me.fiksu.esp32map.companion.domain.RouteStatusMessage
import me.fiksu.esp32map.companion.domain.RouteSyncMessage
import me.fiksu.esp32map.companion.domain.RouteSyncStatusCode
import me.fiksu.esp32map.companion.domain.RouteSummary
import me.fiksu.esp32map.companion.domain.RouteUpdateMessage

object BleRouteSyncGattContract {
    const val SERVICE_UUID: String = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1001"
    const val CHUNK_WRITE_CHARACTERISTIC_UUID: String = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1002"
    const val EVENT_NOTIFY_CHARACTERISTIC_UUID: String = "8d0f3f30-7b4d-4f7c-8b24-2f8e7e4e1003"
}

data class RouteTransferChunkEnvelope(
    val transferIdentifier: String,
    val chunkIndex: Int,
    val totalChunks: Int,
    val checksumHex: String,
    val payloadFragment: ByteArray,
)

sealed interface BleRouteSyncPacket {
    data class Chunk(val chunk: RouteTransferChunkEnvelope) : BleRouteSyncPacket
    data class SyncMessage(val message: RouteSyncMessage) : BleRouteSyncPacket
}

object BleRouteSyncCodec {
    private const val PACKET_VERSION: String = "1"

    fun canonicalPayloadBytes(message: RouteSyncMessage): ByteArray =
        canonicalPayloadString(message).encodeToByteArray()

    fun checksumHex(payload: ByteArray): String {
        var hash = 2_166_136_261L
        payload.forEach { byte ->
            hash = hash xor (byte.toInt() and 0xff).toLong()
            hash = (hash * 16_777_619L) and 0xffff_ffffL
        }
        return "%08x".format(hash)
    }

    fun chunkEnvelopes(
        message: RouteSyncMessage,
        transferIdentifier: String,
        chunkSizeBytes: Int,
    ): List<RouteTransferChunkEnvelope> {
        val payload = canonicalPayloadBytes(message)
        val safeChunkSize = maxOf(chunkSizeBytes, 1)
        val totalChunks = maxOf(1, ceil(payload.size.toDouble() / safeChunkSize.toDouble()).toInt())
        val checksum = checksumHex(payload)
        return payload.asList().chunked(safeChunkSize).mapIndexed { index, fragment ->
            RouteTransferChunkEnvelope(
                transferIdentifier = transferIdentifier,
                chunkIndex = index,
                totalChunks = totalChunks,
                checksumHex = checksum,
                payloadFragment = fragment.toByteArray(),
            )
        }
    }

    fun encode(packet: BleRouteSyncPacket): ByteArray {
        return when (packet) {
            is BleRouteSyncPacket.Chunk -> encodePacket(
                headers = listOf(
                    "v" to PACKET_VERSION,
                    "type" to "chunk",
                    "transfer_id" to packet.chunk.transferIdentifier,
                    "chunk_index" to packet.chunk.chunkIndex.toString(),
                    "total_chunks" to packet.chunk.totalChunks.toString(),
                    "checksum" to packet.chunk.checksumHex,
                ),
                body = packet.chunk.payloadFragment,
            )
            is BleRouteSyncPacket.SyncMessage -> encodePacket(
                headers = listOf(
                    "v" to PACKET_VERSION,
                    "type" to "sync_message",
                ),
                body = canonicalPayloadBytes(packet.message),
            )
        }
    }

    fun decode(payload: ByteArray): BleRouteSyncPacket {
        val (headers, body) = splitHeaderAndBody(payload)
        require(requiredField(headers, "v") == PACKET_VERSION) { "Unsupported packet version" }
        return when (requiredField(headers, "type")) {
            "chunk" -> BleRouteSyncPacket.Chunk(
                RouteTransferChunkEnvelope(
                    transferIdentifier = requiredField(headers, "transfer_id"),
                    chunkIndex = parseInt(requiredField(headers, "chunk_index"), "chunk_index"),
                    totalChunks = parseInt(requiredField(headers, "total_chunks"), "total_chunks"),
                    checksumHex = requiredField(headers, "checksum"),
                    payloadFragment = body,
                ),
            )
            "sync_message" -> BleRouteSyncPacket.SyncMessage(decodeSyncMessage(body))
            else -> error("Unsupported BLE route sync packet type")
        }
    }

    fun decodeSyncMessage(payload: ByteArray): RouteSyncMessage {
        val text = payload.decodeToString()
        val fields = text.lineSequence()
            .mapNotNull { line ->
                val separator = line.indexOf('=')
                if (separator < 0) null else line.substring(0, separator).trim() to line.substring(separator + 1)
            }
            .toMap()
        return when (val kind = requiredField(fields, "kind")) {
            "set" -> RouteSyncMessage.Set(RouteSetMessage(parseRoutePackage(fields)))
            "update" -> {
                val route = parseRoutePackage(fields)
                RouteSyncMessage.Update(
                    RouteUpdateMessage(
                        routeIdentifier = route.routeIdentifier,
                        revision = route.revision,
                        route = route,
                    ),
                )
            }
            "clear" -> RouteSyncMessage.Clear(RouteClearMessage(optionalStringField(fields, "route_id")))
            "status" -> RouteSyncMessage.Status(
                RouteStatusMessage(
                    routeIdentifier = optionalStringField(fields, "route_id"),
                    revision = optionalIntField(fields, "revision"),
                    status = parseStatus(requiredField(fields, "status")),
                    detail = optionalStringField(fields, "detail"),
                ),
            )
            "reroute_request" -> RouteSyncMessage.RerouteRequest(
                RouteRerouteRequestMessage(
                    routeIdentifier = optionalStringField(fields, "route_id") ?: "none",
                    riderLocation = parseCoordinatePoint(requiredField(fields, "rider")),
                    reason = requiredField(fields, "reason"),
                ),
            )
            else -> error("Unsupported sync message kind $kind")
        }
    }

    private fun canonicalPayloadString(message: RouteSyncMessage): String {
        return when (message) {
            is RouteSyncMessage.Set -> routePayloadString("set", message.message.route)
            is RouteSyncMessage.Update -> routePayloadString("update", message.message.route)
            is RouteSyncMessage.Clear -> listOf(
                "kind=clear",
                "route_id=${message.message.routeIdentifier ?: "current"}",
            ).joinToString("\n")
            is RouteSyncMessage.Status -> listOf(
                "kind=status",
                "route_id=${message.message.routeIdentifier ?: "none"}",
                "revision=${message.message.revision ?: "none"}",
                "status=${message.message.status.name.lowercase()}",
                "detail=${message.message.detail.orEmpty()}",
            ).joinToString("\n")
            is RouteSyncMessage.RerouteRequest -> listOf(
                "kind=reroute_request",
                "route_id=${message.message.routeIdentifier}",
                "rider=${"%.6f,%.6f".format(message.message.riderLocation.latitude, message.message.riderLocation.longitude)}",
                "reason=${message.message.reason}",
            ).joinToString("\n")
        }
    }

    private fun routePayloadString(kind: String, route: NormalizedRoutePackage): String {
        val geometry = route.geometry.joinToString(";") { point ->
            "%.6f,%.6f".format(point.latitude, point.longitude)
        }
        val maneuvers = route.maneuvers.joinToString(";") { maneuver ->
            listOf(
                maneuver.id,
                maneuver.maneuverType.name.lowercase(),
                "%.1f".format(maneuver.distanceFromStartMeters),
                maneuver.distanceToNextMeters?.let { "%.1f".format(it) }.orEmpty(),
                "%.6f,%.6f".format(maneuver.location.latitude, maneuver.location.longitude),
                maneuver.instructionText.orEmpty(),
            ).joinToString("|")
        }
        return listOf(
            "kind=$kind",
            "route_id=${route.routeIdentifier}",
            "revision=${route.revision}",
            "version=${route.version.major}.${route.version.minor}",
            "summary=${"%.1f|%d|%s|%s".format(route.summary.totalDistanceMeters, route.summary.estimatedDurationSeconds, route.summary.startLabel.orEmpty(), route.summary.destinationLabel.orEmpty())}",
            "geometry=$geometry",
            "maneuvers=$maneuvers",
            "provenance=${route.provenance.providerId.name.lowercase()}|${route.provenance.sourceReference.orEmpty()}|${route.provenance.generatedAtUnixMs}",
        ).joinToString("\n")
    }

    private fun parseRoutePackage(fields: Map<String, String>): NormalizedRoutePackage {
        val version = requiredField(fields, "version")
        val versionParts = version.split('.', limit = 2)
        require(versionParts.size == 2) { "Invalid version $version" }
        return NormalizedRoutePackage(
            version = RoutePackageVersion(
                major = parseInt(versionParts[0], "version.major"),
                minor = parseInt(versionParts[1], "version.minor"),
            ),
            routeIdentifier = requiredField(fields, "route_id"),
            revision = parseInt(requiredField(fields, "revision"), "revision"),
            geometry = parseGeometry(requiredField(fields, "geometry")),
            maneuvers = parseManeuvers(requiredField(fields, "maneuvers")),
            summary = parseSummary(requiredField(fields, "summary")),
            provenance = parseProvenance(requiredField(fields, "provenance")),
        )
    }

    private fun parseSummary(value: String): RouteSummary {
        val parts = value.split('|', limit = 4)
        require(parts.size == 4) { "Invalid summary $value" }
        return RouteSummary(
            totalDistanceMeters = parseDouble(parts[0], "summary.totalDistanceMeters"),
            estimatedDurationSeconds = parseInt(parts[1], "summary.estimatedDurationSeconds"),
            startLabel = parts[2].ifEmpty { null },
            destinationLabel = parts[3].ifEmpty { null },
        )
    }

    private fun parseGeometry(value: String): List<CoordinatePoint> {
        if (value.isEmpty()) return emptyList()
        return value.split(';').map(::parseCoordinatePoint)
    }

    private fun parseManeuvers(value: String): List<RouteManeuver> {
        if (value.isEmpty()) return emptyList()
        return value.split(';').map { entry ->
            val parts = entry.split('|', limit = 6)
            val distanceFromStart = parseDouble(parts.getOrElse(2) { "" }, "maneuver.distanceFromStartMeters")
            val tuple = when (parts.size) {
                5 -> Triple(null, parts[3], parts[4])
                6 -> Triple(parts[3].ifEmpty { null }?.let { parseDouble(it, "maneuver.distanceToNextMeters") }, parts[4], parts[5])
                else -> error("Invalid maneuver entry $entry")
            }
            RouteManeuver(
                id = parts.firstOrNull().orEmpty(),
                maneuverType = parseManeuverType(parts.getOrElse(1) { "" }),
                location = parseCoordinatePoint(tuple.second),
                distanceFromStartMeters = distanceFromStart,
                distanceToNextMeters = tuple.first,
                instructionText = tuple.third.ifEmpty { null },
            )
        }
    }

    private fun parseProvenance(value: String): RouteProvenance {
        val parts = value.split('|', limit = 3)
        require(parts.size == 3) { "Invalid provenance $value" }
        return RouteProvenance(
            providerId = parseProvider(parts[0]),
            sourceReference = parts[1].ifEmpty { null },
            generatedAtUnixMs = parseLong(parts[2], "provenance.generatedAtUnixMs"),
        )
    }

    private fun parseCoordinatePoint(value: String): CoordinatePoint {
        val parts = value.split(',', limit = 2)
        require(parts.size == 2) { "Invalid coordinate $value" }
        return CoordinatePoint(
            latitude = parseDouble(parts[0], "coordinate.latitude"),
            longitude = parseDouble(parts[1], "coordinate.longitude"),
        )
    }

    private fun parseProvider(value: String): RouteProviderId {
        return when (value.trim()) {
            "hsl", "hsl_digitransit" -> RouteProviderId.HSL
            "osm" -> RouteProviderId.OSM
            "gpx" -> RouteProviderId.GPX_IMPORT
            "fit" -> RouteProviderId.FIT_IMPORT
            "tcx" -> RouteProviderId.TCX_IMPORT
            else -> RouteProviderId.OSM
        }
    }

    private fun parseManeuverType(value: String): RouteManeuverType {
        return when (value.trim()) {
            "depart" -> RouteManeuverType.DEPART
            "straight" -> RouteManeuverType.STRAIGHT
            "slight_left", "slightLeft" -> RouteManeuverType.SLIGHT_LEFT
            "left" -> RouteManeuverType.LEFT
            "sharp_left", "sharpLeft" -> RouteManeuverType.SHARP_LEFT
            "slight_right", "slightRight" -> RouteManeuverType.SLIGHT_RIGHT
            "right" -> RouteManeuverType.RIGHT
            "sharp_right", "sharpRight" -> RouteManeuverType.SHARP_RIGHT
            "uturn", "u_turn" -> RouteManeuverType.UTURN
            "roundabout" -> RouteManeuverType.ROUNDABOUT
            "merge" -> RouteManeuverType.MERGE
            "ramp" -> RouteManeuverType.RAMP
            "arrive" -> RouteManeuverType.ARRIVE
            else -> error("Unsupported maneuver type $value")
        }
    }

    private fun parseStatus(value: String): RouteSyncStatusCode {
        return when (value.trim()) {
            "accepted" -> RouteSyncStatusCode.ACCEPTED
            "applying" -> RouteSyncStatusCode.APPLYING
            "active" -> RouteSyncStatusCode.ACTIVE
            "cleared" -> RouteSyncStatusCode.CLEARED
            "rejected" -> RouteSyncStatusCode.REJECTED
            "retryable_failure", "retryableFailure" -> RouteSyncStatusCode.RETRYABLE_FAILURE
            "fatal_failure", "fatalFailure" -> RouteSyncStatusCode.FATAL_FAILURE
            else -> error("Unsupported route sync status $value")
        }
    }

    private fun encodePacket(headers: List<Pair<String, String>>, body: ByteArray): ByteArray {
        val header = buildString {
            append(headers.joinToString("\n") { (key, value) -> "$key=$value" })
            append("\npayload_length=${body.size}\n\n")
        }
        return header.encodeToByteArray() + body
    }

    private fun splitHeaderAndBody(payload: ByteArray): Pair<Map<String, String>, ByteArray> {
        val separator = payload.indexOfHeaderSeparator()
        require(separator >= 0) { "Missing BLE route sync header separator" }
        val headerText = payload.copyOfRange(0, separator).decodeToString()
        val body = payload.copyOfRange(separator + 2, payload.size)
        val headers = headerText.lineSequence()
            .mapNotNull { line ->
                val index = line.indexOf('=')
                if (index < 0) null else line.substring(0, index).trim() to line.substring(index + 1)
            }
            .toMap()
        val expectedLength = parseInt(requiredField(headers, "payload_length"), "payload_length")
        require(expectedLength == body.size) { "BLE route sync payload length mismatch" }
        return headers to body
    }

    private fun requiredField(fields: Map<String, String>, key: String): String =
        fields[key] ?: error("Missing field $key")

    private fun optionalStringField(fields: Map<String, String>, key: String): String? =
        fields[key]?.takeUnless { it.isEmpty() || it == "none" || it == "current" }

    private fun optionalIntField(fields: Map<String, String>, key: String): Int? =
        fields[key]?.takeUnless { it.isEmpty() || it == "none" }?.let { parseInt(it, key) }

    private fun parseInt(value: String, field: String): Int =
        value.toIntOrNull() ?: error("Invalid integer for $field: $value")

    private fun parseLong(value: String, field: String): Long =
        value.toLongOrNull() ?: error("Invalid long for $field: $value")

    private fun parseDouble(value: String, field: String): Double =
        value.toDoubleOrNull() ?: error("Invalid double for $field: $value")
}

private fun ByteArray.indexOfHeaderSeparator(): Int {
    for (index in 0 until size - 1) {
        if (this[index] == '\n'.code.toByte() && this[index + 1] == '\n'.code.toByte()) {
            return index
        }
    }
    return -1
}
