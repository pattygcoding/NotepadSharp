//! Language detection and the per-language syntax description used by the highlighter.
//!
//! Rather than embedding a grammar engine, each language is described by data: keyword
//! sets, comment tokens and quoting rules. That keeps highlighting fast, dependency-free
//! and trivially testable.

use serde::{Deserialize, Serialize};
use std::path::Path;

/// Languages NotepadSharp can highlight.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Language {
    PlainText,
    Rust,
    Python,
    JavaScript,
    TypeScript,
    C,
    Cpp,
    CSharp,
    Java,
    Go,
    Html,
    Xml,
    Css,
    Json,
    Yaml,
    Toml,
    Markdown,
    Sql,
    Shell,
    Lua,
    Php,
    Ini,
    Batch,
}

/// How the generic lexer should treat a language.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyntaxKind {
    /// C-like: identifiers, numbers, operators, comments.
    Generic,
    /// `<tag attr="value">` markup.
    Markup,
    /// Headings, links, emphasis, code fences.
    Markdown,
    /// `key = value` / `key: value` style formats.
    KeyValue,
}

/// Everything the highlighter needs to know about a language.
#[derive(Clone, Copy, Debug)]
pub struct Syntax {
    pub keywords: &'static [&'static str],
    pub types: &'static [&'static str],
    pub literals: &'static [&'static str],
    pub line_comment: Option<&'static str>,
    pub line_comment_alt: Option<&'static str>,
    pub block_comment: Option<(&'static str, &'static str)>,
    pub quote_chars: &'static [char],
    pub triple_quotes: bool,
    pub kind: SyntaxKind,
    pub case_insensitive: bool,
}

impl Syntax {
    /// Token to insert when toggling a line comment.
    pub fn comment_token(&self) -> &'static str {
        self.line_comment
            .or(self.line_comment_alt)
            .or_else(|| self.block_comment.map(|(start, _)| start))
            .unwrap_or("")
    }
}

macro_rules! syntax {
    ($kw:expr, $ty:expr, $lit:expr, $lc:expr, $alt:expr, $bc:expr, $quotes:expr, $triple:expr, $kind:expr) => {
        Syntax {
            keywords: $kw,
            types: $ty,
            literals: $lit,
            line_comment: $lc,
            line_comment_alt: $alt,
            block_comment: $bc,
            quote_chars: $quotes,
            triple_quotes: $triple,
            kind: $kind,
            case_insensitive: false,
        }
    };
}

impl Language {
    pub const ALL: [Language; 23] = [
        Language::PlainText,
        Language::Rust,
        Language::Python,
        Language::JavaScript,
        Language::TypeScript,
        Language::C,
        Language::Cpp,
        Language::CSharp,
        Language::Java,
        Language::Go,
        Language::Html,
        Language::Xml,
        Language::Css,
        Language::Json,
        Language::Yaml,
        Language::Toml,
        Language::Markdown,
        Language::Sql,
        Language::Shell,
        Language::Lua,
        Language::Php,
        Language::Ini,
        Language::Batch,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Language::PlainText => "Normal text",
            Language::Rust => "Rust",
            Language::Python => "Python",
            Language::JavaScript => "JavaScript",
            Language::TypeScript => "TypeScript",
            Language::C => "C",
            Language::Cpp => "C++",
            Language::CSharp => "C#",
            Language::Java => "Java",
            Language::Go => "Go",
            Language::Html => "HTML",
            Language::Xml => "XML",
            Language::Css => "CSS",
            Language::Json => "JSON",
            Language::Yaml => "YAML",
            Language::Toml => "TOML",
            Language::Markdown => "Markdown",
            Language::Sql => "SQL",
            Language::Shell => "Shell",
            Language::Lua => "Lua",
            Language::Php => "PHP",
            Language::Ini => "INI",
            Language::Batch => "Batch",
        }
    }

    /// Detects a language from a file extension (with or without the leading dot).
    pub fn from_extension(extension: &str) -> Language {
        match extension
            .trim_start_matches('.')
            .to_ascii_lowercase()
            .as_str()
        {
            "rs" => Language::Rust,
            "py" | "pyw" | "pyi" => Language::Python,
            "js" | "mjs" | "cjs" | "jsx" => Language::JavaScript,
            "ts" | "mts" | "cts" | "tsx" => Language::TypeScript,
            "c" | "h" => Language::C,
            "cpp" | "cc" | "cxx" | "hpp" | "hh" | "hxx" | "ino" => Language::Cpp,
            "cs" | "csx" => Language::CSharp,
            "java" => Language::Java,
            "go" => Language::Go,
            "html" | "htm" | "xhtml" | "vue" | "svelte" => Language::Html,
            "xml" | "xsd" | "xsl" | "svg" | "plist" | "csproj" | "props" | "wxs" => Language::Xml,
            "css" | "scss" | "less" => Language::Css,
            "json" | "jsonc" | "json5" => Language::Json,
            "yaml" | "yml" => Language::Yaml,
            "toml" => Language::Toml,
            "md" | "markdown" | "mdx" => Language::Markdown,
            "sql" => Language::Sql,
            "sh" | "bash" | "zsh" | "ksh" | "env" => Language::Shell,
            "lua" => Language::Lua,
            "php" | "php3" | "php4" | "php5" | "phtml" => Language::Php,
            "ini" | "cfg" | "conf" | "properties" | "gitconfig" => Language::Ini,
            "bat" | "cmd" => Language::Batch,
            _ => Language::PlainText,
        }
    }

    /// Detects a language from a path's extension.
    pub fn from_path(path: &Path) -> Language {
        path.extension()
            .and_then(|extension| extension.to_str())
            .map(Language::from_extension)
            .unwrap_or(Language::PlainText)
    }

    /// Detects a language from a `#!` shebang line, if it looks like one.
    pub fn from_shebang(first_line: &str) -> Option<Language> {
        let line = first_line.trim();
        if !line.starts_with("#!") {
            return None;
        }
        let lower = line.to_ascii_lowercase();
        if lower.contains("python") {
            Some(Language::Python)
        } else if lower.contains("node") || lower.contains("deno") {
            Some(Language::JavaScript)
        } else if lower.contains("lua") {
            Some(Language::Lua)
        } else if lower.contains("php") {
            Some(Language::Php)
        } else if lower.contains("bash") || lower.contains("/sh") || lower.contains("zsh") {
            Some(Language::Shell)
        } else {
            None
        }
    }

    /// The data description used by the lexer.
    pub fn syntax(self) -> &'static Syntax {
        match self {
            Language::PlainText => &PLAIN,
            Language::Rust => &RUST,
            Language::Python => &PYTHON,
            Language::JavaScript => &JAVASCRIPT,
            Language::TypeScript => &TYPESCRIPT,
            Language::C => &C,
            Language::Cpp => &CPP,
            Language::CSharp => &CSHARP,
            Language::Java => &JAVA,
            Language::Go => &GO,
            Language::Html => &HTML,
            Language::Xml => &XML,
            Language::Css => &CSS,
            Language::Json => &JSON,
            Language::Yaml => &YAML,
            Language::Toml => &TOML,
            Language::Markdown => &MARKDOWN,
            Language::Sql => &SQL,
            Language::Shell => &SHELL,
            Language::Lua => &LUA,
            Language::Php => &PHP,
            Language::Ini => &INI,
            Language::Batch => &BATCH,
        }
    }

    /// Comment token used by the "toggle comment" line operation.
    pub fn comment_token(self) -> &'static str {
        self.syntax().comment_token()
    }
}

// ---------------------------------------------------------------------------
// Language data
// ---------------------------------------------------------------------------

static RUST_KEYWORDS: &[&str] = &[
    "as",
    "async",
    "await",
    "break",
    "const",
    "continue",
    "crate",
    "dyn",
    "else",
    "enum",
    "extern",
    "fn",
    "for",
    "if",
    "impl",
    "in",
    "let",
    "loop",
    "match",
    "mod",
    "move",
    "mut",
    "pub",
    "ref",
    "return",
    "self",
    "Self",
    "static",
    "struct",
    "super",
    "trait",
    "type",
    "unsafe",
    "use",
    "where",
    "while",
    "macro_rules",
    "union",
];
static RUST_TYPES: &[&str] = &[
    "i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32",
    "f64", "bool", "char", "str", "String", "Vec", "Option", "Result", "Box", "Rc", "Arc",
    "HashMap", "HashSet", "BTreeMap", "Some", "None", "Ok", "Err",
];

static PYTHON_KEYWORDS: &[&str] = &[
    "and", "as", "assert", "async", "await", "break", "class", "continue", "def", "del", "elif",
    "else", "except", "finally", "for", "from", "global", "if", "import", "in", "is", "lambda",
    "nonlocal", "not", "or", "pass", "raise", "return", "try", "while", "with", "yield", "match",
    "case",
];
static PYTHON_TYPES: &[&str] = &[
    "int",
    "float",
    "str",
    "bool",
    "list",
    "dict",
    "set",
    "tuple",
    "bytes",
    "object",
    "Exception",
    "ValueError",
    "TypeError",
    "self",
    "cls",
];

static JS_KEYWORDS: &[&str] = &[
    "var",
    "let",
    "const",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "new",
    "delete",
    "typeof",
    "instanceof",
    "in",
    "of",
    "class",
    "extends",
    "super",
    "this",
    "throw",
    "try",
    "catch",
    "finally",
    "async",
    "await",
    "yield",
    "import",
    "export",
    "from",
    "as",
    "static",
    "get",
    "set",
];
static JS_TYPES: &[&str] = &[
    "Array", "Object", "String", "Number", "Boolean", "Promise", "Map", "Set", "Date", "RegExp",
    "Error", "JSON", "Math", "console", "document", "window",
];

static TS_KEYWORDS: &[&str] = &[
    "var",
    "let",
    "const",
    "function",
    "return",
    "if",
    "else",
    "for",
    "while",
    "do",
    "switch",
    "case",
    "default",
    "break",
    "continue",
    "new",
    "delete",
    "typeof",
    "instanceof",
    "in",
    "of",
    "class",
    "extends",
    "super",
    "this",
    "throw",
    "try",
    "catch",
    "finally",
    "async",
    "await",
    "yield",
    "import",
    "export",
    "from",
    "as",
    "static",
    "get",
    "set",
    "interface",
    "type",
    "enum",
    "implements",
    "readonly",
    "namespace",
    "declare",
    "abstract",
    "public",
    "private",
    "protected",
    "satisfies",
    "keyof",
    "infer",
    "asserts",
    "is",
    "override",
    "unique",
];
static TS_TYPES: &[&str] = &[
    "any",
    "unknown",
    "never",
    "void",
    "undefined",
    "null",
    "string",
    "number",
    "boolean",
    "object",
    "symbol",
    "bigint",
    "Array",
    "Promise",
    "Record",
    "Partial",
    "Readonly",
    "Map",
    "Set",
    "Date",
    "RegExp",
    "Error",
];

static C_KEYWORDS: &[&str] = &[
    "auto",
    "break",
    "case",
    "const",
    "continue",
    "default",
    "do",
    "else",
    "enum",
    "extern",
    "for",
    "goto",
    "if",
    "inline",
    "register",
    "restrict",
    "return",
    "sizeof",
    "static",
    "struct",
    "switch",
    "typedef",
    "union",
    "volatile",
    "while",
    "_Bool",
    "_Static_assert",
    "thread_local",
];
static C_TYPES: &[&str] = &[
    "char", "short", "int", "long", "float", "double", "void", "signed", "unsigned", "size_t",
    "ssize_t", "int8_t", "int16_t", "int32_t", "int64_t", "uint8_t", "uint16_t", "uint32_t",
    "uint64_t", "FILE", "bool",
];

static CPP_KEYWORDS: &[&str] = &[
    "auto",
    "break",
    "case",
    "catch",
    "class",
    "concept",
    "const",
    "constexpr",
    "const_cast",
    "continue",
    "decltype",
    "default",
    "delete",
    "do",
    "dynamic_cast",
    "else",
    "enum",
    "explicit",
    "export",
    "extern",
    "final",
    "for",
    "friend",
    "goto",
    "if",
    "inline",
    "mutable",
    "namespace",
    "new",
    "noexcept",
    "operator",
    "override",
    "private",
    "protected",
    "public",
    "register",
    "reinterpret_cast",
    "requires",
    "return",
    "sizeof",
    "static",
    "static_assert",
    "static_cast",
    "struct",
    "switch",
    "template",
    "throw",
    "try",
    "typedef",
    "typename",
    "union",
    "using",
    "virtual",
    "volatile",
    "while",
];
static CPP_TYPES: &[&str] = &[
    "bool",
    "char",
    "double",
    "float",
    "int",
    "long",
    "short",
    "signed",
    "unsigned",
    "size_t",
    "std",
    "string",
    "vector",
    "map",
    "unordered_map",
    "set",
    "optional",
    "variant",
    "span",
    "unique_ptr",
    "shared_ptr",
    "weak_ptr",
    "thread",
    "mutex",
    "cout",
    "cin",
    "cerr",
    "endl",
];

static CSHARP_KEYWORDS: &[&str] = &[
    "abstract",
    "as",
    "async",
    "await",
    "base",
    "break",
    "case",
    "catch",
    "checked",
    "class",
    "const",
    "continue",
    "default",
    "delegate",
    "do",
    "else",
    "enum",
    "event",
    "explicit",
    "extern",
    "finally",
    "fixed",
    "for",
    "foreach",
    "get",
    "goto",
    "if",
    "implicit",
    "in",
    "init",
    "interface",
    "internal",
    "is",
    "lock",
    "namespace",
    "new",
    "operator",
    "out",
    "override",
    "params",
    "partial",
    "private",
    "protected",
    "public",
    "readonly",
    "record",
    "ref",
    "required",
    "return",
    "sealed",
    "set",
    "sizeof",
    "stackalloc",
    "static",
    "struct",
    "switch",
    "this",
    "throw",
    "try",
    "typeof",
    "unchecked",
    "unsafe",
    "using",
    "var",
    "virtual",
    "volatile",
    "when",
    "where",
    "while",
    "yield",
];
static CSHARP_TYPES: &[&str] = &[
    "bool",
    "byte",
    "char",
    "decimal",
    "double",
    "float",
    "int",
    "long",
    "object",
    "sbyte",
    "short",
    "string",
    "uint",
    "ulong",
    "ushort",
    "void",
    "dynamic",
    "List",
    "Dictionary",
    "Task",
    "Action",
    "Func",
    "Span",
    "Memory",
    "IEnumerable",
    "DateTime",
    "Guid",
];

static JAVA_KEYWORDS: &[&str] = &[
    "abstract",
    "assert",
    "break",
    "case",
    "catch",
    "class",
    "const",
    "continue",
    "default",
    "do",
    "else",
    "enum",
    "extends",
    "final",
    "finally",
    "for",
    "goto",
    "if",
    "implements",
    "import",
    "instanceof",
    "interface",
    "native",
    "new",
    "package",
    "private",
    "protected",
    "public",
    "record",
    "return",
    "sealed",
    "static",
    "strictfp",
    "super",
    "switch",
    "synchronized",
    "this",
    "throw",
    "throws",
    "transient",
    "try",
    "var",
    "volatile",
    "while",
    "yield",
];
static JAVA_TYPES: &[&str] = &[
    "boolean",
    "byte",
    "char",
    "double",
    "float",
    "int",
    "long",
    "short",
    "void",
    "String",
    "Object",
    "Integer",
    "Double",
    "Long",
    "List",
    "ArrayList",
    "Map",
    "HashMap",
    "Set",
    "HashSet",
    "Optional",
    "Stream",
    "Exception",
    "RuntimeException",
];

static GO_KEYWORDS: &[&str] = &[
    "break",
    "case",
    "chan",
    "const",
    "continue",
    "default",
    "defer",
    "else",
    "fallthrough",
    "for",
    "func",
    "go",
    "goto",
    "if",
    "import",
    "interface",
    "map",
    "package",
    "range",
    "return",
    "select",
    "struct",
    "switch",
    "type",
    "var",
];
static GO_TYPES: &[&str] = &[
    "bool",
    "byte",
    "complex64",
    "complex128",
    "error",
    "float32",
    "float64",
    "int",
    "int8",
    "int16",
    "int32",
    "int64",
    "rune",
    "string",
    "uint",
    "uint8",
    "uint16",
    "uint32",
    "uint64",
    "uintptr",
    "any",
];

static SHELL_KEYWORDS: &[&str] = &[
    "if", "then", "else", "elif", "fi", "for", "in", "do", "done", "while", "until", "case",
    "esac", "function", "return", "local", "export", "readonly", "shift", "source", "echo", "cd",
    "exit", "set", "unset", "trap", "eval", "exec", "printf", "read", "test",
];

static LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

static PHP_KEYWORDS: &[&str] = &[
    "abstract",
    "and",
    "array",
    "as",
    "break",
    "callable",
    "case",
    "catch",
    "class",
    "clone",
    "const",
    "continue",
    "declare",
    "default",
    "do",
    "echo",
    "else",
    "elseif",
    "empty",
    "endfor",
    "endforeach",
    "endif",
    "endswitch",
    "endwhile",
    "enum",
    "extends",
    "final",
    "finally",
    "fn",
    "for",
    "foreach",
    "function",
    "global",
    "goto",
    "if",
    "implements",
    "include",
    "include_once",
    "instanceof",
    "insteadof",
    "interface",
    "isset",
    "list",
    "match",
    "namespace",
    "new",
    "or",
    "print",
    "private",
    "protected",
    "public",
    "readonly",
    "require",
    "require_once",
    "return",
    "static",
    "switch",
    "throw",
    "trait",
    "try",
    "unset",
    "use",
    "var",
    "while",
    "xor",
    "yield",
];

static BATCH_KEYWORDS: &[&str] = &[
    "echo", "set", "setlocal", "endlocal", "if", "else", "goto", "call", "for", "in", "do", "exit",
    "cd", "mkdir", "rmdir", "del", "copy", "move", "start", "pause", "shift", "rem", "pushd",
    "popd", "type", "choice", "cls", "title",
];

static SQL_KEYWORDS: &[&str] = &[
    "add",
    "all",
    "alter",
    "and",
    "any",
    "as",
    "asc",
    "begin",
    "between",
    "by",
    "case",
    "cast",
    "check",
    "column",
    "commit",
    "constraint",
    "create",
    "cross",
    "database",
    "default",
    "delete",
    "desc",
    "distinct",
    "drop",
    "else",
    "end",
    "except",
    "exists",
    "foreign",
    "from",
    "full",
    "group",
    "having",
    "if",
    "in",
    "index",
    "inner",
    "insert",
    "intersect",
    "into",
    "is",
    "join",
    "key",
    "left",
    "like",
    "limit",
    "not",
    "null",
    "offset",
    "on",
    "or",
    "order",
    "outer",
    "primary",
    "references",
    "returning",
    "right",
    "rollback",
    "row",
    "select",
    "set",
    "table",
    "then",
    "top",
    "transaction",
    "union",
    "unique",
    "update",
    "values",
    "view",
    "when",
    "where",
    "with",
];

// ---------------------------------------------------------------------------
// Syntax tables
// ---------------------------------------------------------------------------

const PLAIN: Syntax = syntax!(
    &[],
    &[],
    &[],
    None,
    None,
    None,
    &[],
    false,
    SyntaxKind::Generic
);
const RUST: Syntax = syntax!(
    RUST_KEYWORDS,
    RUST_TYPES,
    &["true", "false"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"'],
    false,
    SyntaxKind::Generic
);
const PYTHON: Syntax = syntax!(
    PYTHON_KEYWORDS,
    PYTHON_TYPES,
    &["True", "False", "None"],
    Some("#"),
    None,
    None,
    &['"', '\''],
    true,
    SyntaxKind::Generic
);
const JAVASCRIPT: Syntax = syntax!(
    JS_KEYWORDS,
    JS_TYPES,
    &["true", "false", "null", "undefined", "NaN"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\'', '`'],
    false,
    SyntaxKind::Generic
);
const TYPESCRIPT: Syntax = syntax!(
    TS_KEYWORDS,
    TS_TYPES,
    &["true", "false", "null", "undefined", "NaN"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\'', '`'],
    false,
    SyntaxKind::Generic
);
const C: Syntax = syntax!(
    C_KEYWORDS,
    C_TYPES,
    &["NULL", "true", "false"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const CPP: Syntax = syntax!(
    CPP_KEYWORDS,
    CPP_TYPES,
    &["NULL", "true", "false", "nullptr"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const CSHARP: Syntax = syntax!(
    CSHARP_KEYWORDS,
    CSHARP_TYPES,
    &["true", "false", "null"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const JAVA: Syntax = syntax!(
    JAVA_KEYWORDS,
    JAVA_TYPES,
    &["true", "false", "null"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const GO: Syntax = syntax!(
    GO_KEYWORDS,
    GO_TYPES,
    &["true", "false", "nil", "iota"],
    Some("//"),
    None,
    Some(("/*", "*/")),
    &['"', '`'],
    false,
    SyntaxKind::Generic
);
const HTML: Syntax = syntax!(
    &[],
    &[],
    &[],
    None,
    None,
    Some(("<!--", "-->")),
    &['"', '\''],
    false,
    SyntaxKind::Markup
);
const XML: Syntax = syntax!(
    &[],
    &[],
    &[],
    None,
    None,
    Some(("<!--", "-->")),
    &['"', '\''],
    false,
    SyntaxKind::Markup
);
const CSS: Syntax = syntax!(
    &[],
    &[],
    &[],
    None,
    None,
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::KeyValue
);
const JSON: Syntax = syntax!(
    &[],
    &[],
    &["true", "false", "null"],
    None,
    None,
    None,
    &['"'],
    false,
    SyntaxKind::KeyValue
);
const YAML: Syntax = syntax!(
    &[],
    &[],
    &["true", "false", "null", "yes", "no"],
    Some("#"),
    None,
    None,
    &['"', '\''],
    false,
    SyntaxKind::KeyValue
);
const TOML: Syntax = syntax!(
    &[],
    &[],
    &["true", "false"],
    Some("#"),
    None,
    None,
    &['"', '\''],
    true,
    SyntaxKind::KeyValue
);
const MARKDOWN: Syntax = syntax!(
    &[],
    &[],
    &[],
    None,
    None,
    None,
    &['`'],
    true,
    SyntaxKind::Markdown
);
const SQL: Syntax = Syntax {
    keywords: SQL_KEYWORDS,
    types: &[],
    literals: &["null"],
    line_comment: Some("--"),
    line_comment_alt: None,
    block_comment: Some(("/*", "*/")),
    quote_chars: &['\'', '"'],
    triple_quotes: false,
    kind: SyntaxKind::Generic,
    case_insensitive: true,
};
const SHELL: Syntax = syntax!(
    SHELL_KEYWORDS,
    &[],
    &["true", "false"],
    Some("#"),
    None,
    None,
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const LUA: Syntax = syntax!(
    LUA_KEYWORDS,
    &[],
    &[],
    Some("--"),
    None,
    Some(("--[[", "]]")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const PHP: Syntax = syntax!(
    PHP_KEYWORDS,
    &[],
    &["true", "false", "null"],
    Some("//"),
    Some("#"),
    Some(("/*", "*/")),
    &['"', '\''],
    false,
    SyntaxKind::Generic
);
const INI: Syntax = syntax!(
    &[],
    &[],
    &[],
    Some(";"),
    Some("#"),
    None,
    &['"'],
    false,
    SyntaxKind::KeyValue
);
const BATCH: Syntax = Syntax {
    keywords: BATCH_KEYWORDS,
    types: &[],
    literals: &[],
    line_comment: Some("::"),
    line_comment_alt: None,
    block_comment: None,
    quote_chars: &['"'],
    triple_quotes: false,
    kind: SyntaxKind::Generic,
    case_insensitive: true,
};

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn detects_by_extension() {
        assert_eq!(Language::from_extension("rs"), Language::Rust);
        assert_eq!(Language::from_extension(".RS"), Language::Rust);
        assert_eq!(Language::from_extension("h"), Language::C);
        assert_eq!(Language::from_extension("hpp"), Language::Cpp);
        assert_eq!(Language::from_extension("tsx"), Language::TypeScript);
        assert_eq!(Language::from_extension("yml"), Language::Yaml);
        assert_eq!(Language::from_extension("unknown-ext"), Language::PlainText);
        assert_eq!(Language::from_extension(""), Language::PlainText);
    }

    #[test]
    fn detects_by_path() {
        assert_eq!(
            Language::from_path(&PathBuf::from("src/main.rs")),
            Language::Rust
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("no_extension")),
            Language::PlainText
        );
    }

    #[test]
    fn detects_by_shebang() {
        assert_eq!(
            Language::from_shebang("#!/usr/bin/env python3"),
            Some(Language::Python)
        );
        assert_eq!(Language::from_shebang("#!/bin/bash"), Some(Language::Shell));
        assert_eq!(
            Language::from_shebang("#!/usr/bin/env node"),
            Some(Language::JavaScript)
        );
        assert_eq!(Language::from_shebang("plain text"), None);
    }

    #[test]
    fn comment_tokens_are_usable() {
        assert_eq!(Language::Rust.comment_token(), "//");
        assert_eq!(Language::Python.comment_token(), "#");
        assert_eq!(Language::Sql.comment_token(), "--");
        assert_eq!(Language::Html.comment_token(), "<!--");
        assert_eq!(Language::Lua.comment_token(), "--");
        assert_eq!(Language::PlainText.comment_token(), "");
    }

    #[test]
    fn keyword_tables_are_populated() {
        for language in Language::ALL {
            let syntax = language.syntax();
            assert!(
                syntax.keywords.iter().all(|k| !k.is_empty()),
                "{language:?}"
            );
            assert!(
                syntax.quote_chars.iter().all(|c| c.is_ascii()),
                "{language:?}"
            );
        }
        assert!(Language::Rust.syntax().keywords.contains(&"fn"));
        assert!(Language::Python.syntax().keywords.contains(&"def"));
        assert!(Language::Cpp.syntax().keywords.contains(&"template"));
        assert!(Language::Markdown.syntax().triple_quotes);
        assert!(Language::Sql.syntax().case_insensitive);
    }

    #[test]
    fn language_list_has_unique_entries() {
        for (index, language) in Language::ALL.iter().enumerate() {
            assert!(
                !Language::ALL[..index].contains(language),
                "duplicate {language:?}"
            );
        }
        assert_eq!(Language::ALL.len(), 23);
    }
}
