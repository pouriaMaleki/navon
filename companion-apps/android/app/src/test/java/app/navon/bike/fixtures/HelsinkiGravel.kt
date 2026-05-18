package app.navon.bike.fixtures

import java.io.File

/**
 * Load the pre-baked helsinki-gravel fixture from
 * `parity-fixtures/data/helsinki-gravel/stream.jsonl`. The path is resolved
 * relative to the gradle module root so tests don't need a resources setup.
 */
object HelsinkiGravel {
    data class Sample(
        val latitude: Double,
        val longitude: Double,
        val speedMps: Double,
        val courseRad: Double,
        val accuracyM: Double,
        val timeOffsetMs: Long,
    )

    fun loadStream(): List<Sample> {
        val root = File(System.getProperty("user.dir") ?: ".").absoluteFile
        // user.dir for a JVM test is `companion-android/app` — go up to repo root.
        val candidates = listOf(
            File(root, "../../../data/parity-fixtures/data/helsinki-gravel/stream.jsonl"),
            File(root, "../../parity-fixtures/data/helsinki-gravel/stream.jsonl"),
            File(root, "../parity-fixtures/data/helsinki-gravel/stream.jsonl"),
            File(root, "parity-fixtures/data/helsinki-gravel/stream.jsonl"),
        )
        val file = candidates.firstOrNull { it.exists() }
            ?: error("helsinki-gravel stream.jsonl missing; run `cargo run -p xtask --bin gen-gps-fixtures`")
        return file.readLines().filter { it.isNotBlank() }.map(::parseLine)
    }

    private fun parseLine(line: String): Sample {
        val lat = readNumber(line, "lat_deg")
        val lon = readNumber(line, "lon_deg")
        val speed = readNumber(line, "speed_mps")
        val course = readNumber(line, "course_rad")
        val accuracy = readNumber(line, "accuracy_m")
        val t = readNumber(line, "t_ms")
        return Sample(
            latitude = lat,
            longitude = lon,
            speedMps = speed,
            courseRad = course,
            accuracyM = accuracy,
            timeOffsetMs = t.toLong(),
        )
    }

    private fun readNumber(line: String, key: String): Double {
        val needle = "\"$key\":"
        val start = line.indexOf(needle)
        require(start >= 0) { "key $key not found in fixture line" }
        val after = line.substring(start + needle.length)
        val end = after.indexOfFirst { it == ',' || it == '}' }
        require(end >= 0) { "key $key not terminated" }
        return after.substring(0, end).trim().toDouble()
    }
}
