// Minimal ICU MessageFormat resolver — supports the subset used by the
// companion app catalog:
//   {name}                         — string passthrough
//   {name, number}                 — locale-aware number formatting
//   {name, select, k1 {a1} other {a2}}  — branch on string value
//
// Plurals / nested complex selects are not implemented because the
// catalog doesn't use them. If we ever need them, swap this for
// `intl-messageformat` and delete this module.

export type MessageValues = Record<string, string | number>;

export function formatMessage(
  template: string,
  values: MessageValues,
  locale: string,
): string {
  const parts = parse(template);
  return parts.map((p) => render(p, values, locale)).join("");
}

type Part =
  | { kind: "text"; value: string }
  | { kind: "var"; name: string }
  | { kind: "number"; name: string }
  | { kind: "select"; name: string; arms: Record<string, Part[]> };

function parse(input: string): Part[] {
  const parts: Part[] = [];
  let i = 0;
  let buf = "";
  while (i < input.length) {
    const ch = input[i];
    if (ch === "'" && input[i + 1] === "{") {
      // ICU literal-quoting: '{ — emit a literal `{` and skip.
      buf += "{";
      i += 2;
      continue;
    }
    if (ch === "{") {
      if (buf) {
        parts.push({ kind: "text", value: buf });
        buf = "";
      }
      const end = findMatchingBrace(input, i);
      const inner = input.slice(i + 1, end).trim();
      parts.push(parsePlaceholder(inner));
      i = end + 1;
      continue;
    }
    buf += ch;
    i++;
  }
  if (buf) parts.push({ kind: "text", value: buf });
  return parts;
}

function parsePlaceholder(inner: string): Part {
  // Forms: `name` | `name, number` | `name, select, k1 {a1} k2 {a2} other {ax}`
  const firstComma = inner.indexOf(",");
  if (firstComma === -1) {
    return { kind: "var", name: inner };
  }
  const name = inner.slice(0, firstComma).trim();
  const rest = inner.slice(firstComma + 1).trim();
  const secondComma = rest.indexOf(",");
  const type = (secondComma === -1 ? rest : rest.slice(0, secondComma)).trim();
  if (type === "number") {
    return { kind: "number", name };
  }
  if (type === "select") {
    const armsBody = rest.slice(secondComma + 1);
    const arms: Record<string, Part[]> = {};
    let i = 0;
    while (i < armsBody.length) {
      while (i < armsBody.length && /\s/.test(armsBody[i])) i++;
      const armNameStart = i;
      while (i < armsBody.length && !/\s/.test(armsBody[i]) && armsBody[i] !== "{") i++;
      const armName = armsBody.slice(armNameStart, i).trim();
      while (i < armsBody.length && armsBody[i] !== "{") i++;
      if (armsBody[i] !== "{") break;
      const end = findMatchingBrace(armsBody, i);
      const armBody = armsBody.slice(i + 1, end);
      arms[armName] = parse(armBody);
      i = end + 1;
    }
    return { kind: "select", name, arms };
  }
  // Unknown type — treat as plain var.
  return { kind: "var", name };
}

function findMatchingBrace(s: string, openIdx: number): number {
  let depth = 0;
  for (let i = openIdx; i < s.length; i++) {
    if (s[i] === "{") depth++;
    else if (s[i] === "}") {
      depth--;
      if (depth === 0) return i;
    }
  }
  throw new Error(`unbalanced braces in ICU template at offset ${openIdx}: ${s}`);
}

function render(part: Part, values: MessageValues, locale: string): string {
  switch (part.kind) {
    case "text":
      return part.value;
    case "var": {
      const v = values[part.name];
      return v === undefined ? `{${part.name}}` : String(v);
    }
    case "number": {
      const v = values[part.name];
      if (v === undefined) return `{${part.name}}`;
      const n = typeof v === "number" ? v : Number(v);
      return Number.isFinite(n) ? new Intl.NumberFormat(locale).format(n) : String(v);
    }
    case "select": {
      const v = values[part.name];
      const key = v === undefined ? "other" : String(v);
      const armParts = part.arms[key] ?? part.arms.other ?? [];
      return armParts.map((p) => render(p, values, locale)).join("");
    }
  }
}
