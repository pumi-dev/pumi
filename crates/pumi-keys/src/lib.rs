pub mod encoder;
pub mod mouse;
pub mod names;
pub use encoder::{Action, Encoder, OptionAsAlt, Options, kitty};
pub use libghostty_vt::key::Key;
pub use names::{
    Chord, MOD_ALT, MOD_CTRL, MOD_SHIFT, MOD_SUPER, MODIFIER_BITS, chord, is_modifier, modifier_key, parse,
};
pub fn encode_names(names: &[String], terminal: &libghostty_vt::Terminal<'_, '_>) -> Result<Vec<u8>, String> {
    let chords = names.iter().map(|name| parse(name).ok_or_else(|| name.clone())).collect::<Result<Vec<_>, _>>()?;
    let mut encoder = Encoder::new().map_err(|err| err.to_string())?;
    let options = Options { option_as_alt: OptionAsAlt::Both, ..Options::from_terminal(terminal) };
    Ok(chords.iter().flat_map(|chord| encoder.encode(chord, &options)).collect())
}
pub fn encode_names_with(names: &[String], options: &Options) -> Result<Vec<u8>, String> {
    let chords = names.iter().map(|name| parse(name).ok_or_else(|| name.clone())).collect::<Result<Vec<_>, _>>()?;
    let mut encoder = Encoder::new().map_err(|err| err.to_string())?;
    Ok(chords.iter().flat_map(|chord| encoder.encode(chord, options)).collect())
}
pub fn options_from_modes(modes: &serde_json::Value) -> Options {
    let dec =
        |mode: u64| modes["dec"].as_array().into_iter().flatten().any(|m| m[0].as_u64() == Some(mode) && m[1] == true);
    Options {
        cursor_keys_application: dec(1),
        keypad_application: modes["keypad"] == true,
        modify_other_keys_2: modes["modify_other_keys"].as_u64() == Some(2),
        kitty_flags: modes["kitty"].as_array().and_then(|k| k.last()).and_then(|f| f.as_u64()).unwrap_or(0) as u8,
        option_as_alt: OptionAsAlt::Both,
        ..Options::default()
    }
}
