//! Explicit message lookup shared with the embedded browser viewer.
use serde::{Deserialize, Serialize};
use std::sync::OnceLock;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Language {
    #[default]
    #[serde(rename = "en")]
    English,
    #[serde(rename = "ru")]
    Russian,
    #[serde(rename = "fr")]
    French,
    #[serde(rename = "de")]
    German,
    #[serde(rename = "es")]
    Spanish,
    #[serde(rename = "it")]
    Italian,
}
impl Language {
    pub const ALL: [Self; 6] = [
        Self::English,
        Self::Russian,
        Self::French,
        Self::German,
        Self::Spanish,
        Self::Italian,
    ];
    pub fn code(self) -> &'static str {
        match self {
            Self::English => "en",
            Self::Russian => "ru",
            Self::French => "fr",
            Self::German => "de",
            Self::Spanish => "es",
            Self::Italian => "it",
        }
    }
    pub fn native_name(self) -> &'static str {
        match self {
            Self::English => "English",
            Self::Russian => "Русский",
            Self::French => "Français",
            Self::German => "Deutsch",
            Self::Spanish => "Español",
            Self::Italian => "Italiano",
        }
    }
}
fn catalog() -> &'static serde_json::Value {
    static CATALOG: OnceLock<serde_json::Value> = OnceLock::new();
    CATALOG.get_or_init(|| {
        serde_json::from_str(include_str!("locales.json")).expect("embedded translation catalog")
    })
}
pub fn tr(language: Language, key: &str) -> &str {
    catalog()
        .get(language.code())
        .and_then(|entries| entries.get(key))
        .and_then(|value| value.as_str())
        .unwrap_or(key)
}
pub fn format(language: Language, key: &str, values: &[(&str, &str)]) -> String {
    let mut output = String::new();
    let mut remaining = tr(language, key);
    while let Some(start) = remaining.find('{') {
        output.push_str(&remaining[..start]);
        let Some(end) = remaining[start..].find('}').map(|offset| start + offset) else {
            output.push_str(&remaining[start..]);
            return output;
        };
        let name = &remaining[start + 1..end];
        if let Some((_, value)) = values.iter().find(|(candidate, _)| *candidate == name) {
            output.push_str(value);
        } else {
            output.push_str(&remaining[start..=end]);
        }
        remaining = &remaining[end + 1..];
    }
    output.push_str(remaining);
    output
}
#[derive(Default)]
pub struct Message {
    key: &'static str,
    values: Vec<(&'static str, String)>,
}
impl Message {
    pub fn new(key: &'static str) -> Self {
        Self {
            key,
            values: Vec::new(),
        }
    }
    pub fn with(key: &'static str, name: &'static str, value: impl Into<String>) -> Self {
        Self {
            key,
            values: vec![(name, value.into())],
        }
    }
    pub fn render(&self, language: Language) -> String {
        format(
            language,
            self.key,
            &self
                .values
                .iter()
                .map(|(name, value)| {
                    (
                        *name,
                        if *name == "detail" {
                            tr(language, value)
                        } else {
                            value.as_str()
                        },
                    )
                })
                .collect::<Vec<_>>(),
        )
    }
    pub fn clear(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeSet;
    fn placeholders(text: &str) -> BTreeSet<&str> {
        text.split('{')
            .skip(1)
            .filter_map(|piece| piece.split_once('}').map(|(name, _)| name))
            .collect()
    }
    #[test]
    fn every_language_covers_the_same_messages_and_preserves_parameters() {
        let english = catalog()["en"].as_object().unwrap();
        for language in Language::ALL {
            let entries = catalog()[language.code()].as_object().unwrap();
            assert_eq!(entries.len(), english.len(), "{}", language.code());
            for (key, value) in english {
                let translated = entries
                    .get(key)
                    .and_then(|value| value.as_str())
                    .expect(key);
                assert!(!translated.trim().is_empty(), "{}: {key}", language.code());
                assert_eq!(
                    placeholders(value.as_str().unwrap()),
                    placeholders(translated),
                    "{}: {key}",
                    language.code()
                );
            }
        }
    }
    #[test]
    fn language_switch_reformats_status_without_translating_user_data() {
        let message = Message::with("Tablet address: {peer}", "peer", "192.168.1.42");
        assert_eq!(
            message.render(Language::English),
            "Tablet address: 192.168.1.42"
        );
        assert_eq!(
            message.render(Language::Russian),
            "Адрес планшета: 192.168.1.42"
        );
        assert_eq!(
            tr(Language::French, "Unknown technical detail"),
            "Unknown technical detail"
        );
        assert_eq!(
            format(
                Language::English,
                "{width} × {height}",
                &[("width", "{height}"), ("height", "720")]
            ),
            "{height} × 720"
        );
    }
}
