//! Guest-side line protocol. Paths are sent as data, never shell code.
pub const GUEST_LOOP: &str = r#"while IFS= read -r p; do
  [ -n "$p" ] || continue
  [ -L "$p" ] && continue
  if [ -e "$p" ]; then
    m=$(stat -c %a -- "$p" 2>/dev/null) || continue
    chmod "$m" -- "$p" 2>/dev/null || true
  fi
done"#;
