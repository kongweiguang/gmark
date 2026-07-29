// @author kongweiguang

/// Built-in Simplified Chinese language identifier.
pub const BUILTIN_LANGUAGE_ZH_CN_ID: &str = "zh-CN";
/// Built-in English language identifier.
pub const BUILTIN_LANGUAGE_EN_US_ID: &str = "en-US";
/// The language selected when no known locale or configured language matches.
pub const DEFAULT_LANGUAGE_ID: &str = BUILTIN_LANGUAGE_EN_US_ID;

/// Normalizes a platform locale enough for deterministic language selection.
///
/// Encoding (for example `.UTF-8`) and modifier suffixes (for example
/// `@calendar=...`) are intentionally discarded, matching the legacy
/// platform-locale behavior.
pub fn normalize_locale(locale: &str) -> Option<String> {
    let locale = locale.trim();
    if locale.is_empty() {
        return None;
    }

    let without_encoding = locale
        .split_once('.')
        .map_or(locale, |(locale, _encoding)| locale);
    let without_modifier = without_encoding
        .split_once('@')
        .map_or(without_encoding, |(locale, _modifier)| locale);
    let normalized = without_modifier.replace('_', "-");
    let language = normalized.split('-').next()?;
    if language.is_empty()
        || !language
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return None;
    }

    let suffix = &normalized[language.len()..];
    Some(format!("{}{}", language.to_ascii_lowercase(), suffix))
}

/// Selects a built-in language id from ordered platform locale preferences.
pub fn language_id_for_locale_preferences<I, S>(locales: I) -> &'static str
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    locales
        .into_iter()
        .find_map(|locale| language_id_for_locale(locale.as_ref()))
        .unwrap_or(DEFAULT_LANGUAGE_ID)
}

pub(crate) fn is_builtin_language_id(language_id: &str) -> bool {
    matches!(
        language_id,
        BUILTIN_LANGUAGE_ZH_CN_ID | BUILTIN_LANGUAGE_EN_US_ID
    )
}

fn language_id_for_locale(locale: &str) -> Option<&'static str> {
    let normalized = normalize_locale(locale)?;
    let language = normalized.split('-').next()?;
    match language {
        "zh" => Some(BUILTIN_LANGUAGE_ZH_CN_ID),
        "en" => Some(BUILTIN_LANGUAGE_EN_US_ID),
        _ => None,
    }
}
