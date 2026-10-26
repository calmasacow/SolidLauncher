//! Read a value out of an application bundle's `Info.plist`.
//!
//! Enough of the format to answer one question: which version is installed. SolidCraft's
//! bundle carries an XML plist, so this reads XML and says so plainly when handed the
//! binary form rather than guessing at it.
//!
//! Compiled under `cfg(test)` everywhere, so the parsing is covered by the suite wherever
//! it runs rather than only on a macOS runner.

/// The value of the first `<string>` after `<key>name</key>`.
///
/// Returns `None` when the key is absent, when the file is a binary plist, or when the
/// document is cut short. A plist is a file on disk that another program wrote, so none of
/// those is an error worth stopping over: the caller reports nothing installed.
pub(super) fn string_value(plist: &str, name: &str) -> Option<String> {
    // A binary plist starts with this magic. Reading it as text would find nothing, or
    // worse, something that looks right.
    if plist.starts_with("bplist00") {
        return None;
    }
    let key = format!("<key>{name}</key>");
    let after_key = plist.split_once(&key)?.1;
    let open = after_key.find("<string>")?;
    // Another key before the value means this key has no string: do not read on into the
    // next one's value and report it as this one's.
    if after_key[..open].contains("<key>") {
        return None;
    }
    let value = &after_key[open + "<string>".len()..];
    let end = value.find("</string>")?;
    Some(unescape(value[..end].trim()))
}

/// The five XML entities, which a generated plist may contain in a path or a name.
fn unescape(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&apos;", "'")
        // Last: an escaped ampersand must not be re-read as the start of another entity.
        .replace("&amp;", "&")
}

#[cfg(test)]
mod tests {
    use super::string_value;

    const PLIST: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<plist version="1.0">
<dict>
  <key>CFBundleExecutable</key>
  <string>SolidCraft</string>
  <key>CFBundleIdentifier</key>
  <string>ai.storyteller.solidcraft</string>
  <key>CFBundleShortVersionString</key>
  <string>0.22.1</string>
  <key>LSMinimumSystemVersion</key>
  <string>11.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
</dict>
</plist>
"#;

    #[test]
    fn reads_the_value_that_belongs_to_its_key() {
        assert_eq!(
            string_value(PLIST, "CFBundleShortVersionString").as_deref(),
            Some("0.22.1")
        );
        assert_eq!(
            string_value(PLIST, "CFBundleIdentifier").as_deref(),
            Some("ai.storyteller.solidcraft")
        );
        assert_eq!(string_value(PLIST, "CFBundleName"), None);
    }

    #[test]
    fn a_key_with_no_string_does_not_borrow_the_next_ones_value() {
        // NSHighResolutionCapable is a <true/>, and the next string belongs to a later key.
        assert_eq!(string_value(PLIST, "NSHighResolutionCapable"), None);
    }

    #[test]
    fn refuses_a_binary_plist_rather_than_reading_rubbish_out_of_it() {
        assert_eq!(
            string_value(
                "bplist00\u{0}\u{1}CFBundleShortVersionString9.9.9",
                "CFBundleShortVersionString"
            ),
            None
        );
    }

    #[test]
    fn survives_a_document_that_is_cut_short() {
        assert_eq!(string_value("<key>A</key><string>unterminated", "A"), None);
        assert_eq!(string_value("", "A"), None);
        assert_eq!(string_value("<key>A</key>", "A"), None);
    }

    #[test]
    fn turns_xml_entities_back_into_their_characters() {
        let plist =
            "<key>A</key><string>Tom &amp; Jerry &lt;1&gt; &quot;x&quot; &apos;y&apos;</string>";
        assert_eq!(
            string_value(plist, "A").as_deref(),
            Some("Tom & Jerry <1> \"x\" 'y'")
        );
        // &amp;lt; is a literal "&lt;", not a "<".
        assert_eq!(
            string_value("<key>A</key><string>&amp;lt;</string>", "A").as_deref(),
            Some("&lt;")
        );
    }
}
