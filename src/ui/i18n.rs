use std::collections::HashMap;

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum LanguageId {
    EnUs,
    ZhCn,
}

impl LanguageId {
    pub const ALL: [Self; 2] = [Self::EnUs, Self::ZhCn];

    pub fn storage_key(self) -> &'static str {
        match self {
            Self::EnUs => "en_us",
            Self::ZhCn => "zh_cn",
        }
    }

    pub fn from_storage_key(key: &str) -> Self {
        match key {
            "zh_cn" => Self::ZhCn,
            _ => Self::EnUs,
        }
    }

    pub fn native_name(self) -> &'static str {
        match self {
            Self::EnUs => "English",
            Self::ZhCn => "简体中文",
        }
    }
}

pub struct Language {
    texts: HashMap<String, String>,
}

impl Language {
    pub fn load(language_id: LanguageId) -> Self {
        let source = match language_id {
            LanguageId::EnUs => include_str!("../../languages/en_us.json"),
            LanguageId::ZhCn => include_str!("../../languages/zh_cn.json"),
        };
        let texts = serde_json::from_str(source).unwrap_or_else(|_| HashMap::new());
        Self { texts }
    }

    pub fn text<'a>(&'a self, key: &'a str) -> &'a str {
        self.texts.get(key).map(String::as_str).unwrap_or(key)
    }
}
