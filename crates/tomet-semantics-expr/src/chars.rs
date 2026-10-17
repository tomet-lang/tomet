//! The names `emoji(...)` knows.

/// The emoji a name stands for, with its aliases. `None` for a name this
/// table does not know. One table, read by this module and by `eval`.
pub(crate) fn emoji_for(name: &str) -> Option<&'static str> {
    Some(match name {
        "sparkles" | "sparkle" => "✨",
        "tada" | "party" => "🎉",
        "check" | "white_check_mark" => "✅",
        "warning" | "warn" => "⚠️",
        "fire" | "flame" => "🔥",
        "rocket" => "🚀",
        "smile" | "grinning" => "😊",
        "heart" => "❤️",
        "star" => "⭐",
        "bulb" | "idea" => "💡",
        "memo" | "pencil" => "📝",
        "gear" | "cog" => "⚙️",
        "link" => "🔗",
        "book" => "📖",
        "pin" | "pushpin" => "📌",
        "wave" => "👋",
        "bug" => "🐛",
        "cross" | "x" => "❌",
        "info" | "information_source" => "ℹ️",
        "eyes" => "👀",
        "clap" => "👏",
        "thumbsup" | "+1" => "👍",
        "thumbsdown" | "-1" => "👎",
        "zap" | "lightning" => "⚡",
        "art" => "🎨",
        "construction" | "wip" => "🚧",
        "lock" => "🔒",
        "unlock" => "🔓",
        "key" => "🔑",
        "mag" | "search" => "🔍",
        "sun" => "☀️",
        "moon" => "🌙",
        "coffee" => "☕",
        _ => return None,
    })
}
