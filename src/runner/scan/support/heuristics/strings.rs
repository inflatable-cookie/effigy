pub(super) fn mask_string_literals(raw_line: &str) -> String {
    let bytes = raw_line.as_bytes();
    let mut out = String::with_capacity(raw_line.len());
    let mut index = 0usize;
    let mut in_single = false;
    let mut in_double = false;
    let mut in_backtick = false;
    let mut raw_hashes = None::<usize>;
    let mut escaped = false;

    while index < bytes.len() {
        if let Some(hash_count) = raw_hashes {
            if bytes[index] == b'"'
                && index + 1 + hash_count <= bytes.len()
                && bytes[index + 1..index + 1 + hash_count]
                    .iter()
                    .all(|byte| *byte == b'#')
            {
                out.push(' ');
                for _ in 0..hash_count {
                    out.push(' ');
                }
                index += 1 + hash_count;
                raw_hashes = None;
                continue;
            }
            out.push(' ');
            index += 1;
            continue;
        }

        let ch = raw_line[index..].chars().next().expect("valid utf-8 char");
        let ch_len = ch.len_utf8();

        if in_single || in_double || in_backtick {
            if escaped {
                escaped = false;
                for _ in 0..ch_len {
                    out.push(' ');
                }
                index += ch_len;
                continue;
            }
            if ch == '\\' && (in_single || in_double || in_backtick) {
                escaped = true;
                out.push(' ');
                index += 1;
                continue;
            }
            if (in_single && ch == '\'') || (in_double && ch == '"') || (in_backtick && ch == '`') {
                in_single = false;
                in_double = false;
                in_backtick = false;
                for _ in 0..ch_len {
                    out.push(' ');
                }
                index += ch_len;
                continue;
            }
            for _ in 0..ch_len {
                out.push(' ');
            }
            index += ch_len;
            continue;
        }

        if let Some((consumed, hashes)) = raw_string_prefix(bytes, index) {
            for _ in 0..consumed {
                out.push(' ');
            }
            index += consumed;
            raw_hashes = Some(hashes);
            continue;
        }

        match ch {
            '\'' => {
                in_single = true;
                out.push(' ');
                index += 1;
            }
            '"' => {
                in_double = true;
                out.push(' ');
                index += 1;
            }
            '`' => {
                in_backtick = true;
                out.push(' ');
                index += 1;
            }
            _ => {
                out.push(ch);
                index += ch_len;
            }
        }
    }

    out
}

fn raw_string_prefix(bytes: &[u8], index: usize) -> Option<(usize, usize)> {
    let start = index;
    let mut cursor = index;

    if bytes.get(cursor) == Some(&b'b') && bytes.get(cursor + 1) == Some(&b'r') {
        cursor += 2;
    } else if bytes.get(cursor) == Some(&b'r') {
        cursor += 1;
    } else {
        return None;
    }

    let mut hashes = 0usize;
    while bytes.get(cursor) == Some(&b'#') {
        hashes += 1;
        cursor += 1;
    }

    if bytes.get(cursor) != Some(&b'"') {
        return None;
    }

    Some((cursor - start + 1, hashes))
}
