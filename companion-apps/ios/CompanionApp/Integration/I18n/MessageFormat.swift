// Minimal ICU MessageFormat resolver — supports the subset used by the
// companion app catalog:
//   {name}                                                — string passthrough
//   {name, number}                                        — locale-aware formatting
//   {name, select, k1 {a1} k2 {a2} other {ax}}            — branch on string value
//
// Plurals / nested complex selects are not implemented because the
// catalog doesn't use them. Mirrors the TypeScript resolver in
// companion-web/src/i18n/messageFormat.ts so all three platforms format
// identically.

import Foundation

enum MessageValue {
    case string(String)
    case number(Double)
}

enum MessageFormat {
    static func format(_ template: String, values: [String: MessageValue], locale: Locale) -> String {
        let parts = parse(template)
        return parts.map { render(part: $0, values: values, locale: locale) }.joined()
    }

    fileprivate enum Part {
        case text(String)
        case variable(name: String)
        case number(name: String)
        case select(name: String, arms: [String: [Part]])
    }

    private static func parse(_ input: String) -> [Part] {
        var parts: [Part] = []
        let chars = Array(input)
        var i = 0
        var buf = ""
        while i < chars.count {
            let ch = chars[i]
            if ch == "'" && i + 1 < chars.count && chars[i + 1] == "{" {
                buf.append("{")
                i += 2
                continue
            }
            if ch == "{" {
                if !buf.isEmpty {
                    parts.append(.text(buf))
                    buf = ""
                }
                let end = findMatchingBrace(chars, openIdx: i)
                let inner = String(chars[(i + 1)..<end]).trimmingCharacters(in: .whitespaces)
                parts.append(parsePlaceholder(inner))
                i = end + 1
                continue
            }
            buf.append(ch)
            i += 1
        }
        if !buf.isEmpty { parts.append(.text(buf)) }
        return parts
    }

    private static func parsePlaceholder(_ inner: String) -> Part {
        guard let firstComma = inner.firstIndex(of: ",") else {
            return .variable(name: inner)
        }
        let name = String(inner[inner.startIndex..<firstComma]).trimmingCharacters(in: .whitespaces)
        let rest = String(inner[inner.index(after: firstComma)...]).trimmingCharacters(in: .whitespaces)
        let secondComma = rest.firstIndex(of: ",")
        let type: String
        if let c = secondComma {
            type = String(rest[rest.startIndex..<c]).trimmingCharacters(in: .whitespaces)
        } else {
            type = rest
        }
        if type == "number" {
            return .number(name: name)
        }
        if type == "select", let c = secondComma {
            let armsBody = String(rest[rest.index(after: c)...])
            return .select(name: name, arms: parseArms(armsBody))
        }
        return .variable(name: name)
    }

    private static func parseArms(_ body: String) -> [String: [Part]] {
        let chars = Array(body)
        var arms: [String: [Part]] = [:]
        var i = 0
        while i < chars.count {
            while i < chars.count && chars[i].isWhitespace { i += 1 }
            let nameStart = i
            while i < chars.count && !chars[i].isWhitespace && chars[i] != "{" {
                i += 1
            }
            let armName = String(chars[nameStart..<i]).trimmingCharacters(in: .whitespaces)
            while i < chars.count && chars[i] != "{" { i += 1 }
            if i >= chars.count || chars[i] != "{" { break }
            let end = findMatchingBrace(chars, openIdx: i)
            let armBody = String(chars[(i + 1)..<end])
            arms[armName] = parse(armBody)
            i = end + 1
        }
        return arms
    }

    private static func findMatchingBrace(_ chars: [Character], openIdx: Int) -> Int {
        var depth = 0
        var i = openIdx
        while i < chars.count {
            if chars[i] == "{" { depth += 1 }
            else if chars[i] == "}" {
                depth -= 1
                if depth == 0 { return i }
            }
            i += 1
        }
        return chars.count - 1
    }

    private static func render(part: Part, values: [String: MessageValue], locale: Locale) -> String {
        switch part {
        case .text(let t):
            return t
        case .variable(let name):
            switch values[name] {
            case .some(.string(let s)): return s
            case .some(.number(let n)):
                return formatNumber(n, locale: locale)
            case .none:
                return "{\(name)}"
            }
        case .number(let name):
            switch values[name] {
            case .some(.string(let s)):
                if let n = Double(s) { return formatNumber(n, locale: locale) }
                return s
            case .some(.number(let n)):
                return formatNumber(n, locale: locale)
            case .none:
                return "{\(name)}"
            }
        case .select(let name, let arms):
            let key: String
            switch values[name] {
            case .some(.string(let s)): key = s
            case .some(.number(let n)): key = String(n)
            case .none: key = "other"
            }
            let armParts = arms[key] ?? arms["other"] ?? []
            return armParts.map { render(part: $0, values: values, locale: locale) }.joined()
        }
    }

    private static var numberFormatters: [String: NumberFormatter] = [:]

    private static func formatNumber(_ n: Double, locale: Locale) -> String {
        let key = locale.identifier
        let nf: NumberFormatter
        if let cached = numberFormatters[key] {
            nf = cached
        } else {
            nf = NumberFormatter()
            nf.locale = locale
            nf.minimumFractionDigits = 0
            numberFormatters[key] = nf
        }
        nf.maximumFractionDigits = (n.truncatingRemainder(dividingBy: 1) == 0) ? 0 : 3
        return nf.string(from: NSNumber(value: n)) ?? String(n)
    }
}
