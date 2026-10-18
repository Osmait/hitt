import SwiftUI

/// Minimal XML/HTML highlighter: colors tags, attribute names, attribute
/// values, and comments. Text outside tags stays in the default color.
enum XmlHighlighter {
    static func highlight(_ input: String) -> AttributedString {
        var out = AttributedString()
        var tokenizer = Tokenizer(input: input)
        while let token = tokenizer.next() {
            switch token {
            case .text(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = SyntaxColors.xmlText
                out.append(chunk)
            case .comment(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = SyntaxColors.xmlComment
                out.append(chunk)
            case .tag(let parts):
                out.append(renderTag(parts))
            }
        }
        return out
    }

    private static func renderTag(_ parts: TagParts) -> AttributedString {
        var out = AttributedString()
        // Opening `<` or `</`
        var open = AttributedString(parts.openingAngle)
        open.foregroundColor = SyntaxColors.jsonPunctuation
        out.append(open)

        var name = AttributedString(parts.name)
        name.foregroundColor = SyntaxColors.xmlTag
        out.append(name)

        for attr in parts.attributes {
            var leading = AttributedString(" ")
            leading.foregroundColor = SyntaxColors.xmlText
            out.append(leading)

            var attrName = AttributedString(attr.name)
            attrName.foregroundColor = SyntaxColors.xmlAttribute
            out.append(attrName)

            if let value = attr.value {
                var eq = AttributedString("=")
                eq.foregroundColor = SyntaxColors.jsonPunctuation
                out.append(eq)

                var val = AttributedString(value)
                val.foregroundColor = SyntaxColors.xmlAttrValue
                out.append(val)
            }
        }

        var close = AttributedString(parts.closingAngle)
        close.foregroundColor = SyntaxColors.jsonPunctuation
        out.append(close)

        return out
    }

    // MARK: - Tokenizer

    private enum Token {
        case text(String)
        case comment(String)
        case tag(TagParts)
    }

    private struct Attr {
        let name: String
        let value: String?  // includes surrounding quotes if present
    }

    private struct TagParts {
        let openingAngle: String   // "<" or "</"
        let name: String
        let attributes: [Attr]
        let closingAngle: String   // ">" or "/>"
    }

    private struct Tokenizer {
        let chars: [Character]
        var index: Int = 0

        init(input: String) { self.chars = Array(input) }

        mutating func next() -> Token? {
            guard index < chars.count else { return nil }

            if chars[index] != "<" {
                return .text(consumeText())
            }
            // Comment?
            if matches("<!--") {
                return .comment(consumeComment())
            }
            return .tag(consumeTag())
        }

        mutating func consumeText() -> String {
            let start = index
            while index < chars.count, chars[index] != "<" { index += 1 }
            return String(chars[start..<index])
        }

        mutating func consumeComment() -> String {
            let start = index
            index += 4 // "<!--"
            while index + 2 < chars.count,
                  !(chars[index] == "-" && chars[index + 1] == "-" && chars[index + 2] == ">")
            {
                index += 1
            }
            // Skip the "-->" if present
            if index + 2 < chars.count { index += 3 } else { index = chars.count }
            return String(chars[start..<index])
        }

        mutating func consumeTag() -> TagParts {
            let openStart = index
            index += 1 // '<'
            let opening: String
            if index < chars.count, chars[index] == "/" {
                index += 1
                opening = "</"
            } else {
                opening = "<"
            }
            _ = openStart

            // Name
            let nameStart = index
            while index < chars.count,
                  !chars[index].isWhitespace,
                  chars[index] != "/",
                  chars[index] != ">"
            {
                index += 1
            }
            let name = String(chars[nameStart..<index])

            // Attributes
            var attrs: [Attr] = []
            while index < chars.count {
                // skip whitespace
                while index < chars.count, chars[index].isWhitespace { index += 1 }
                if index >= chars.count || chars[index] == "/" || chars[index] == ">" {
                    break
                }
                // attr name
                let aStart = index
                while index < chars.count,
                      !chars[index].isWhitespace,
                      chars[index] != "=",
                      chars[index] != "/",
                      chars[index] != ">"
                {
                    index += 1
                }
                let aName = String(chars[aStart..<index])
                var aValue: String? = nil
                if index < chars.count, chars[index] == "=" {
                    index += 1
                    let vStart = index
                    if index < chars.count, chars[index] == "\"" || chars[index] == "'" {
                        let quote = chars[index]
                        index += 1
                        while index < chars.count, chars[index] != quote { index += 1 }
                        if index < chars.count { index += 1 }
                    } else {
                        while index < chars.count,
                              !chars[index].isWhitespace,
                              chars[index] != ">",
                              chars[index] != "/"
                        {
                            index += 1
                        }
                    }
                    aValue = String(chars[vStart..<index])
                }
                if !aName.isEmpty {
                    attrs.append(Attr(name: aName, value: aValue))
                }
            }

            // Closing '/>' or '>'
            var closing = ">"
            if index < chars.count, chars[index] == "/" {
                closing = "/>"
                index += 1
            }
            if index < chars.count, chars[index] == ">" { index += 1 }

            return TagParts(openingAngle: opening, name: name, attributes: attrs, closingAngle: closing)
        }

        func matches(_ literal: String) -> Bool {
            let lc = Array(literal)
            guard index + lc.count <= chars.count else { return false }
            for i in 0..<lc.count where chars[index + i] != lc[i] { return false }
            return true
        }
    }
}
