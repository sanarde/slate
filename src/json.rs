// SPDX-License-Identifier: GPL-3.0-only

pub fn quote(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('"');
    for character in text.chars() {
        match character {
            '"' => quoted.push_str("\\\""),
            '\\' => quoted.push_str("\\\\"),
            '\n' => quoted.push_str("\\n"),
            '\r' => quoted.push_str("\\r"),
            '\t' => quoted.push_str("\\t"),
            control if (control as u32) < 0x20 => quoted.push_str(&format!("\\u{:04x}", control as u32)),
            other => quoted.push(other),
        }
    }
    quoted.push('"');
    quoted
}

pub fn object(fields: &[(&str, String)]) -> String {
    let body: Vec<String> = fields
        .iter()
        .map(|(name, value)| format!("{}:{}", quote(name), value))
        .collect();
    format!("{{{}}}", body.join(","))
}

pub fn array(items: &[String]) -> String {
    format!("[{}]", items.join(","))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_are_escaped() {
        assert_eq!(quote("a\"b\\c\nd\u{1}"), "\"a\\\"b\\\\c\\nd\\u0001\"");
        assert_eq!(quote("Привет"), "\"Привет\"");
    }

    #[test]
    fn objects_and_arrays_nest() {
        let inner = object(&[("id", "1".to_string()), ("title", quote("x"))]);
        assert_eq!(array(&[inner]), "[{\"id\":1,\"title\":\"x\"}]");
    }
}
