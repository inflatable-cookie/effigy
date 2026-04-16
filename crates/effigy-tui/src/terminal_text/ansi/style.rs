use ratatui::style::{Color, Modifier, Style};

pub(super) fn apply_sgr(current: Style, sgr: &str, base: Style) -> Style {
    let mut style = current;
    let params = if sgr.is_empty() {
        vec!["0"]
    } else {
        sgr.split(';').collect::<Vec<&str>>()
    };
    let mut index = 0usize;
    while index < params.len() {
        match params[index].parse::<u16>() {
            Ok(0) => style = base,
            Ok(1) => style = style.add_modifier(Modifier::BOLD),
            Ok(2) => style = style.add_modifier(Modifier::DIM),
            Ok(3) => style = style.add_modifier(Modifier::ITALIC),
            Ok(4) => style = style.add_modifier(Modifier::UNDERLINED),
            Ok(22) => style = style.remove_modifier(Modifier::BOLD | Modifier::DIM),
            Ok(23) => style = style.remove_modifier(Modifier::ITALIC),
            Ok(24) => style = style.remove_modifier(Modifier::UNDERLINED),
            Ok(30) => style = style.fg(Color::Black),
            Ok(31) => style = style.fg(Color::Red),
            Ok(32) => style = style.fg(Color::Green),
            Ok(33) => style = style.fg(Color::Yellow),
            Ok(34) => style = style.fg(Color::Blue),
            Ok(35) => style = style.fg(Color::Magenta),
            Ok(36) => style = style.fg(Color::Cyan),
            Ok(37) => style = style.fg(Color::Gray),
            Ok(39) => style = style.fg(base.fg.unwrap_or(Color::Reset)),
            Ok(40) => style = style.bg(Color::Black),
            Ok(41) => style = style.bg(Color::Red),
            Ok(42) => style = style.bg(Color::Green),
            Ok(43) => style = style.bg(Color::Yellow),
            Ok(44) => style = style.bg(Color::Blue),
            Ok(45) => style = style.bg(Color::Magenta),
            Ok(46) => style = style.bg(Color::Cyan),
            Ok(47) => style = style.bg(Color::Gray),
            Ok(49) => style = style.bg(base.bg.unwrap_or(Color::Reset)),
            Ok(90) => style = style.fg(Color::DarkGray),
            Ok(91) => style = style.fg(Color::LightRed),
            Ok(92) => style = style.fg(Color::LightGreen),
            Ok(93) => style = style.fg(Color::LightYellow),
            Ok(94) => style = style.fg(Color::LightBlue),
            Ok(95) => style = style.fg(Color::LightMagenta),
            Ok(96) => style = style.fg(Color::LightCyan),
            Ok(97) => style = style.fg(Color::White),
            Ok(100) => style = style.bg(Color::DarkGray),
            Ok(101) => style = style.bg(Color::LightRed),
            Ok(102) => style = style.bg(Color::LightGreen),
            Ok(103) => style = style.bg(Color::LightYellow),
            Ok(104) => style = style.bg(Color::LightBlue),
            Ok(105) => style = style.bg(Color::LightMagenta),
            Ok(106) => style = style.bg(Color::LightCyan),
            Ok(107) => style = style.bg(Color::White),
            Ok(38) => {
                if let Some((color, consumed)) = parse_extended_color(&params[index + 1..]) {
                    style = style.fg(color);
                    index += consumed;
                }
            }
            Ok(48) => {
                if let Some((color, consumed)) = parse_extended_color(&params[index + 1..]) {
                    style = style.bg(color);
                    index += consumed;
                }
            }
            _ => {}
        }
        index += 1;
    }
    style
}

fn parse_extended_color(params: &[&str]) -> Option<(Color, usize)> {
    let mode = params.first()?.parse::<u16>().ok()?;
    match mode {
        5 => {
            let index = params.get(1)?.parse::<u8>().ok()?;
            Some((Color::Indexed(index), 2))
        }
        2 => {
            let red = params.get(1)?.parse::<u8>().ok()?;
            let green = params.get(2)?.parse::<u8>().ok()?;
            let blue = params.get(3)?.parse::<u8>().ok()?;
            Some((Color::Rgb(red, green, blue), 4))
        }
        _ => None,
    }
}
