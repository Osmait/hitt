import SwiftUI

/// Extended palette for syntax highlighting. Themes extend the base
/// `ThemePalette` via computed accessors that derive from existing colors.
enum SyntaxColors {
    static var jsonKey: Color { HittTheme.Colors.accent }
    static var jsonString: Color { HittTheme.Colors.success }
    static var jsonNumber: Color { HittTheme.Colors.warning }
    static var jsonBoolean: Color { HittTheme.Colors.accentHover }
    static var jsonNull: Color { HittTheme.Colors.textTertiary }
    static var jsonPunctuation: Color { HittTheme.Colors.textSecondary }
    static var jsonError: Color { HittTheme.Colors.danger }

    // XML / HTML
    static var xmlTag: Color { HittTheme.Colors.accent }
    static var xmlAttribute: Color { HittTheme.Colors.warning }
    static var xmlAttrValue: Color { HittTheme.Colors.success }
    static var xmlText: Color { HittTheme.Colors.textPrimary }
    static var xmlComment: Color { HittTheme.Colors.textTertiary }
}

/// Hand-rolled single-pass JSON tokenizer that emits an `AttributedString`
/// with theme-driven colors. Handles: object/array/string/number/bool/null,
/// plus whitespace and punctuation. If the input is not valid JSON the
/// highlighter still renders sensible tokens up to the first error.
enum JsonHighlighter {
    static func highlight(_ input: String) -> AttributedString {
        var out = AttributedString()
        var tokenizer = Tokenizer(input: input)
        var pendingKeyCandidate = false
        // A string is a key iff the next non-ws non-comment character is ':'
        // We lookahead after each string token to decide.
        while let token = tokenizer.next() {
            switch token {
            case .whitespace(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = HittTheme.Colors.textPrimary
                out.append(chunk)
            case .punctuation(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = SyntaxColors.jsonPunctuation
                out.append(chunk)
                pendingKeyCandidate = (s == "{" || s == ",")
            case .string(let raw):
                let color: Color
                if pendingKeyCandidate && tokenizer.peekNonWhitespace() == ":" {
                    color = SyntaxColors.jsonKey
                } else {
                    color = SyntaxColors.jsonString
                }
                var chunk = AttributedString(raw)
                chunk.foregroundColor = color
                out.append(chunk)
                pendingKeyCandidate = false
            case .number(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = SyntaxColors.jsonNumber
                out.append(chunk)
                pendingKeyCandidate = false
            case .keyword(let s):
                let color: Color = (s == "null") ? SyntaxColors.jsonNull : SyntaxColors.jsonBoolean
                var chunk = AttributedString(s)
                chunk.foregroundColor = color
                out.append(chunk)
                pendingKeyCandidate = false
            case .unknown(let s):
                var chunk = AttributedString(s)
                chunk.foregroundColor = SyntaxColors.jsonError
                out.append(chunk)
            }
        }
        return out
    }

    // MARK: - Tokenizer

    private enum Token {
        case whitespace(String)
        case punctuation(String)   // one of { } [ ] : ,
        case string(String)        // includes the surrounding quotes
        case number(String)
        case keyword(String)       // true / false / null
        case unknown(String)       // any other byte (for robustness)
    }

    private struct Tokenizer {
        let chars: [Character]
        var index: Int = 0

        init(input: String) { self.chars = Array(input) }

        mutating func next() -> Token? {
            guard index < chars.count else { return nil }
            let c = chars[index]

            if c.isWhitespace {
                let start = index
                while index < chars.count, chars[index].isWhitespace { index += 1 }
                return .whitespace(String(chars[start..<index]))
            }
            if c == "\"" {
                return .string(consumeString())
            }
            if c == "-" || c.isNumber {
                return .number(consumeNumber())
            }
            if c.isLetter {
                let start = index
                while index < chars.count, chars[index].isLetter { index += 1 }
                let word = String(chars[start..<index])
                if word == "true" || word == "false" || word == "null" {
                    return .keyword(word)
                }
                return .unknown(word)
            }
            if "{}[]:,".contains(c) {
                let out = String(c); index += 1
                return .punctuation(out)
            }
            // Fallback — single unknown char
            let out = String(c); index += 1
            return .unknown(out)
        }

        mutating func consumeString() -> String {
            let start = index
            index += 1 // opening quote
            while index < chars.count {
                let c = chars[index]
                if c == "\\" {
                    index += 1
                    if index < chars.count { index += 1 }
                    continue
                }
                if c == "\"" {
                    index += 1
                    break
                }
                index += 1
            }
            return String(chars[start..<index])
        }

        mutating func consumeNumber() -> String {
            let start = index
            // Optional leading '-'
            if chars[index] == "-" { index += 1 }
            while index < chars.count {
                let c = chars[index]
                if c.isNumber || c == "." || c == "e" || c == "E" || c == "+" || c == "-" {
                    index += 1
                } else {
                    break
                }
            }
            return String(chars[start..<index])
        }

        /// Look at the next non-whitespace character without advancing.
        func peekNonWhitespace() -> Character? {
            var i = index
            while i < chars.count, chars[i].isWhitespace { i += 1 }
            return i < chars.count ? chars[i] : nil
        }
    }
}
