package app.navon.bike.integration.i18n

import java.text.NumberFormat
import java.util.Locale

/**
 * Minimal ICU MessageFormat resolver — supports the subset used by the
 * companion app catalog:
 *   {name}                                              — string passthrough
 *   {name, number}                                      — locale-aware formatting
 *   {name, select, k1 {a1} k2 {a2} other {ax}}          — branch on string value
 *
 * Plurals / nested complex selects are not implemented because the catalog
 * doesn't use them. Mirrors the TypeScript and Swift resolvers so all three
 * platforms format identically.
 */
object MessageFormat {
    fun format(template: String, values: Map<String, Any>, locale: Locale): String =
        parse(template).joinToString("") { render(it, values, locale) }

    private sealed interface Part {
        data class Text(val value: String) : Part
        data class Variable(val name: String) : Part
        data class Number(val name: String) : Part
        data class Select(val name: String, val arms: Map<String, List<Part>>) : Part
    }

    private fun parse(input: String): List<Part> {
        val parts = mutableListOf<Part>()
        val chars = input.toCharArray()
        var i = 0
        val buf = StringBuilder()
        while (i < chars.size) {
            val ch = chars[i]
            if (ch == '\'' && i + 1 < chars.size && chars[i + 1] == '{') {
                buf.append('{')
                i += 2
                continue
            }
            if (ch == '{') {
                if (buf.isNotEmpty()) {
                    parts += Part.Text(buf.toString())
                    buf.setLength(0)
                }
                val end = findMatchingBrace(chars, i)
                val inner = String(chars, i + 1, end - i - 1).trim()
                parts += parsePlaceholder(inner)
                i = end + 1
                continue
            }
            buf.append(ch)
            i++
        }
        if (buf.isNotEmpty()) parts += Part.Text(buf.toString())
        return parts
    }

    private fun parsePlaceholder(inner: String): Part {
        val firstComma = inner.indexOf(',')
        if (firstComma < 0) return Part.Variable(inner)
        val name = inner.substring(0, firstComma).trim()
        val rest = inner.substring(firstComma + 1).trim()
        val secondComma = rest.indexOf(',')
        val type = if (secondComma < 0) rest else rest.substring(0, secondComma).trim()
        return when (type) {
            "number" -> Part.Number(name)
            "select" -> {
                if (secondComma < 0) Part.Variable(name)
                else Part.Select(name, parseArms(rest.substring(secondComma + 1)))
            }
            else -> Part.Variable(name)
        }
    }

    private fun parseArms(body: String): Map<String, List<Part>> {
        val chars = body.toCharArray()
        val arms = mutableMapOf<String, List<Part>>()
        var i = 0
        while (i < chars.size) {
            while (i < chars.size && chars[i].isWhitespace()) i++
            val nameStart = i
            while (i < chars.size && !chars[i].isWhitespace() && chars[i] != '{') i++
            val armName = String(chars, nameStart, i - nameStart).trim()
            while (i < chars.size && chars[i] != '{') i++
            if (i >= chars.size || chars[i] != '{') break
            val end = findMatchingBrace(chars, i)
            val armBody = String(chars, i + 1, end - i - 1)
            arms[armName] = parse(armBody)
            i = end + 1
        }
        return arms
    }

    private fun findMatchingBrace(chars: CharArray, openIdx: Int): Int {
        var depth = 0
        var i = openIdx
        while (i < chars.size) {
            when (chars[i]) {
                '{' -> depth++
                '}' -> {
                    depth--
                    if (depth == 0) return i
                }
            }
            i++
        }
        return chars.size - 1
    }

    private fun render(part: Part, values: Map<String, Any>, locale: Locale): String = when (part) {
        is Part.Text -> part.value
        is Part.Variable -> values[part.name]?.toString() ?: "{${part.name}}"
        is Part.Number -> {
            val v = values[part.name]
            when (v) {
                null -> "{${part.name}}"
                is Number -> NumberFormat.getNumberInstance(locale).format(v)
                else -> v.toString().toDoubleOrNull()?.let {
                    NumberFormat.getNumberInstance(locale).format(it)
                } ?: v.toString()
            }
        }
        is Part.Select -> {
            val key = values[part.name]?.toString() ?: "other"
            val armParts = part.arms[key] ?: part.arms["other"] ?: emptyList()
            armParts.joinToString("") { render(it, values, locale) }
        }
    }
}
