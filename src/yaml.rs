//! A tiny YAML writer for the generated Compose file: block maps, lists of
//! scalars, and scalars quoted whenever plain style could be misread.

pub enum Yaml {
    Map(Vec<(String, Yaml)>),
    List(Vec<Yaml>),
    /// A string, quoted if needed.
    Str(String),
    /// Written as is: numbers and booleans.
    Raw(String),
}

pub fn map<K: Into<String>>(entries: impl IntoIterator<Item = (K, Yaml)>) -> Yaml {
    Yaml::Map(entries.into_iter().map(|(k, v)| (k.into(), v)).collect())
}

pub fn list(items: impl IntoIterator<Item = Yaml>) -> Yaml {
    Yaml::List(items.into_iter().collect())
}

pub fn s(value: impl Into<String>) -> Yaml {
    Yaml::Str(value.into())
}

impl Yaml {
    /// Renders a top-level map as a YAML document.
    pub fn render(&self) -> String {
        let mut out = String::new();
        emit(&mut out, self, 0);
        out
    }
}

fn emit(out: &mut String, yaml: &Yaml, indent: usize) {
    let Yaml::Map(entries) = yaml else {
        out.push_str(&scalar(yaml));
        out.push('\n');
        return;
    };
    for (key, value) in entries {
        out.push_str(&" ".repeat(indent));
        out.push_str(&quote_key(key));
        out.push(':');
        match value {
            Yaml::Map(inner) if inner.is_empty() => out.push_str(" {}\n"),
            Yaml::List(items) if items.is_empty() => out.push_str(" []\n"),
            Yaml::Map(_) => {
                out.push('\n');
                emit(out, value, indent + 2);
            }
            Yaml::List(items) => {
                out.push('\n');
                for item in items {
                    out.push_str(&" ".repeat(indent + 2));
                    out.push_str("- ");
                    out.push_str(&scalar(item));
                    out.push('\n');
                }
            }
            _ => {
                out.push(' ');
                out.push_str(&scalar(value));
                out.push('\n');
            }
        }
    }
}

fn scalar(yaml: &Yaml) -> String {
    match yaml {
        Yaml::Str(text) => quote(text),
        Yaml::Raw(text) => text.clone(),
        // Nested collections inside lists are not needed by the Compose file.
        Yaml::Map(_) | Yaml::List(_) => unreachable!("only scalars in lists"),
    }
}

fn quote_key(key: &str) -> String {
    let plain = !key.is_empty()
        && key.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'));
    if plain { key.to_string() } else { json_quote(key) }
}

/// Plain style only for strings that can't be read as anything else
/// (numbers, booleans, null, `7000:7000` sexagesimals, comments, flow syntax).
fn quote(text: &str) -> String {
    let first_ok = text.chars().next().is_some_and(|c| c.is_ascii_alphabetic() || c == '/' || c == '.');
    let chars_ok = text.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '@' | '='));
    let keyword = matches!(
        text.to_ascii_lowercase().as_str(),
        "true" | "false" | "yes" | "no" | "on" | "off" | "y" | "n" | "null" | ".inf" | ".nan"
    );
    if first_ok && chars_ok && !keyword && !text.ends_with(':') { text.to_string() } else { json_quote(text) }
}

fn json_quote(text: &str) -> String {
    serde_json::to_string(text).expect("strings serialize")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_block_style() {
        let doc = map([
            ("name", s("rin-my-team")),
            ("services", map([("hub", map([
                ("ports", list([s("127.0.0.1:10302:7000")])),
                ("init", Yaml::Raw("true".into())),
                ("command", list([s("rin"), s("hub")])),
                ("environment", map([("A", s("on")), ("B", s("4g")), ("C", s("http://hub:7000"))])),
                ("empty", map(Vec::<(String, Yaml)>::new())),
            ]))])),
        ]);
        assert_eq!(
            doc.render(),
            "name: rin-my-team\nservices:\n  hub:\n    ports:\n      - \"127.0.0.1:10302:7000\"\n    init: true\n    command:\n      - rin\n      - hub\n    environment:\n      A: \"on\"\n      B: \"4g\"\n      C: http://hub:7000\n    empty: {}\n"
        );
    }

    #[test]
    fn quotes_anything_ambiguous() {
        for text in ["", "true", "No", "2", "0.5", "7000:7000", "a b", "#x", "x: y", "-x", "{a}", "a\"b", "ü"] {
            assert!(quote(text).starts_with('"'), "{text:?} should be quoted");
        }
        for text in ["rin", "/home/agent", "./env/rin.env", "maximhq/bifrost:v1.2.3", "http://hub:7000"] {
            assert_eq!(quote(text), text);
        }
    }
}
