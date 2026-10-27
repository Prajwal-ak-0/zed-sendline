//! Interpolate request and environment values. Expansion never performs IO.
use anyhow::{Context, Result, bail};
use std::collections::HashMap;
const MAX_EXPANDED_BYTES: usize = 10 * 1024 * 1024;

pub fn resolve_variables(text: &str, variables: &HashMap<String, String>) -> Result<String> {
    fn resolve(
        text: &str,
        variables: &HashMap<String, String>,
        stack: &mut Vec<String>,
    ) -> Result<String> {
        let mut result = String::new();
        let mut tail = text;
        while let Some(start) = tail.find("{{") {
            result.push_str(&tail[..start]);
            let remaining = &tail[start + 2..];
            let end = remaining
                .find("}}")
                .context("Unclosed variable placeholder")?;
            let key = remaining[..end].trim();
            let value = match key {
                "$uuid" | "$guid" => uuid::Uuid::new_v4().to_string(),
                "$timestamp" => chrono::Utc::now().timestamp().to_string(),
                "$datetime" | "$datetime iso8601" => chrono::Utc::now().to_rfc3339(),
                "$datetime rfc1123" => chrono::Utc::now().to_rfc2822(),
                _ => {
                    if stack.iter().any(|item| item == key) || stack.len() >= 32 {
                        bail!("Circular or excessively nested variable: {key}");
                    }
                    let value = variable(key, variables).with_context(|| {
                        format!(
                            "Undefined variable: {key}. Select an environment or define @{key}."
                        )
                    })?;
                    stack.push(key.to_string());
                    let resolved = resolve(&value, variables, stack)?;
                    stack.pop();
                    resolved
                }
            };
            result.push_str(&value);
            if result.len() > MAX_EXPANDED_BYTES {
                bail!("Expanded request exceeds 10 MiB");
            }
            tail = &remaining[end + 2..];
        }
        result.push_str(tail);
        if result.len() > MAX_EXPANDED_BYTES {
            bail!("Expanded request exceeds 10 MiB");
        }
        Ok(result)
    }
    resolve(text, variables, &mut Vec::new())
}

/// Resolve a literal name first, then a JSON property/index path over a saved value.
fn variable(key: &str, variables: &HashMap<String, String>) -> Option<String> {
    if let Some(value) = variables.get(key) {
        return Some(value.clone());
    }
    let split = key.find(['.', '['])?;
    let mut value: serde_json::Value = serde_json::from_str(variables.get(&key[..split])?).ok()?;
    let mut path = &key[split..];
    while !path.is_empty() {
        if let Some(rest) = path.strip_prefix('.') {
            let end = rest.find(['.', '[']).unwrap_or(rest.len());
            value = value.get(&rest[..end])?.clone();
            path = &rest[end..];
        } else {
            let rest = path.strip_prefix('[')?;
            // JSON's parser handles quoted keys, including escaped quotes and brackets.
            let mut stream =
                serde_json::Deserializer::from_str(rest).into_iter::<serde_json::Value>();
            let index = stream.next()?.ok()?;
            path = rest[stream.byte_offset()..]
                .trim_start()
                .strip_prefix(']')?;
            value = match index {
                serde_json::Value::String(name) => value.get(&name)?.clone(),
                serde_json::Value::Number(number) => {
                    value.get(usize::try_from(number.as_u64()?).ok()?)?.clone()
                }
                _ => return None,
            };
        }
    }
    Some(
        value
            .as_str()
            .map(str::to_string)
            .unwrap_or_else(|| value.to_string()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_json_paths_without_evaluating_expressions() {
        let variables = HashMap::from([
            (
                "mdmUsers".into(),
                r#"[{"_id":"user-1","profile":{"name":"नमस्ते"},"x]y":"bracket"}]"#.into(),
            ),
            ("literal.name".into(), "literal".into()),
        ]);
        assert_eq!(
            resolve_variables(r#"/users/{{mdmUsers[0]["_id"]}}"#, &variables).unwrap(),
            "/users/user-1"
        );
        assert_eq!(
            resolve_variables("{{mdmUsers[0].profile.name}}", &variables).unwrap(),
            "नमस्ते"
        );
        assert_eq!(
            resolve_variables(r#"{{mdmUsers[0]["x]y"]}}"#, &variables).unwrap(),
            "bracket"
        );
        assert_eq!(
            resolve_variables("{{literal.name}}", &variables).unwrap(),
            "literal"
        );
        for key in [
            "mdmUsers[9]._id",
            "mdmUsers[0].missing",
            "mdmUsers[0].profile()",
            "mdmUsers[true]",
        ] {
            assert!(resolve_variables(&format!("{{{{{key}}}}}"), &variables).is_err());
        }
    }
}
