//! What an enchanting table's option says: the level it needs and the enchantment it hints at
//! (the game shows the hint in the option's tooltip, under galactic text).

/// Bedrock's enchantment ids, in order.
const NAMES: [&str; 42] = [
    "Protection", "Fire Protection", "Feather Falling", "Blast Protection", "Projectile Protection", "Thorns", "Respiration",
    "Depth Strider", "Aqua Affinity", "Sharpness", "Smite", "Bane of Arthropods", "Knockback", "Fire Aspect", "Looting",
    "Efficiency", "Silk Touch", "Unbreaking", "Fortune", "Power", "Punch", "Flame", "Infinity", "Luck of the Sea", "Lure",
    "Frost Walker", "Mending", "Curse of Binding", "Curse of Vanishing", "Impaling", "Riptide", "Loyalty", "Channeling",
    "Multishot", "Piercing", "Quick Charge", "Soul Speed", "Swift Sneak", "Wind Burst", "Density", "Breach", "Lunge",
];
const ROMAN: [&str; 5] = ["I", "II", "III", "IV", "V"];
/// Letters of a name that fit the option's button beside the level.
const FIT: usize = 13;

/// `level` is what the option needs; `hint` is an enchantment's id and level.
pub fn option(level: u8, hint: Option<(u8, u8)>) -> String {
    let Some((name, rank)) = hint.and_then(|(id, rank)| Some((*NAMES.get(usize::from(id))?, rank))) else { return format!("Level {level}") };
    let short: String = name.chars().take(FIT).collect();
    let cut = if short.len() < name.len() { "." } else { "" };
    format!("{level}: {short}{cut} {}...", ROMAN.get(usize::from(rank).saturating_sub(1)).copied().unwrap_or("?"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_option_names_its_level_and_its_hint() {
        assert_eq!(option(30, Some((9, 4))), "30: Sharpness IV...");
        assert_eq!(option(8, Some((4, 1))), "8: Projectile Pr. I...");
        assert_eq!((option(3, None), option(3, Some((200, 1)))), ("Level 3".to_owned(), "Level 3".to_owned()));
    }
}
