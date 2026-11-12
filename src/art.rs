pub const BANNER: &str = "CRAB RAVE";
pub const GLYPH_HEIGHT: i32 = 7;

pub fn glyph(ch: char) -> &'static [&'static str; 7] {
    match ch {
        'C' => &[
            " ### ", "#   #", "#    ", "#    ", "#    ", "#   #", " ### ",
        ],
        'R' => &[
            "#### ", "#   #", "#   #", "#### ", "# #  ", "#  # ", "#   #",
        ],
        'A' => &[
            " ### ", "#   #", "#   #", "#####", "#   #", "#   #", "#   #",
        ],
        'B' => &[
            "#### ", "#   #", "#   #", "#### ", "#   #", "#   #", "#### ",
        ],
        'V' => &[
            "#   #", "#   #", "#   #", "#   #", "#   #", " # # ", "  #  ",
        ],
        'E' => &[
            "#####", "#    ", "#    ", "#### ", "#    ", "#    ", "#####",
        ],
        _ => &["   "; 7],
    }
}

/// Banner width in font pixels, with one blank column between characters.
pub fn banner_width() -> i32 {
    BANNER
        .chars()
        .map(|ch| glyph(ch)[0].len() as i32 + 1)
        .sum::<i32>()
        - 1
}
