//! Environment variables: `{{name}}` placeholders resolved before a manual
//! request is sent. The active environment overrides global variables; the
//! engine receives the merged list and resolves every field of the request
//! (URL, query, headers, body, auth) so the stored transaction shows exactly
//! what went on the wire.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::model::{AuthSpec, ManualBody, ManualRequest, MultipartField};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Variable {
    pub name: String,
    pub value: String,
    /// Secret values are masked in the editor; the wire value is unaffected.
    pub is_secret: bool,
}

/// Replaces every `{{name}}` occurrence with the variable's value in a single
/// pass, so variable values can never chain-expand into each other. Unknown
/// names are left untouched for the composer to flag.
pub fn resolve(text: &str, variables: &[Variable]) -> String {
    let by_name: HashMap<&str, &str> = variables
        .iter()
        .map(|variable| (variable.name.as_str(), variable.value.as_str()))
        .collect();
    resolve_with(text, &by_name)
}

fn resolve_with(text: &str, by_name: &HashMap<&str, &str>) -> String {
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        output.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        if let Some(end) = after.find("}}") {
            let name = &after[..end];
            if let Some(value) = by_name.get(name) {
                output.push_str(value);
            } else {
                output.push_str(&format!("{{{{{name}}}}}"));
            }
            rest = &after[end + 2..];
        } else {
            output.push_str(&rest[start..]);
            rest = "";
        }
    }
    output.push_str(rest);
    output
}

/// A resolved copy of the request; the original stays untouched so the
/// composer keeps showing the `{{name}}` placeholders. The lookup table is
/// built once per request instead of per field.
pub fn resolve_request(request: &ManualRequest, variables: &[Variable]) -> ManualRequest {
    let by_name: HashMap<&str, &str> = variables
        .iter()
        .map(|variable| (variable.name.as_str(), variable.value.as_str()))
        .collect();
    let resolve = |text: &str| resolve_with(text, &by_name);
    let mut resolved = request.clone();
    resolved.url = resolve(&request.url);
    for entry in &mut resolved.query {
        entry.name = resolve(&entry.name);
        entry.value = resolve(&entry.value);
    }
    for entry in &mut resolved.headers {
        entry.name = resolve(&entry.name);
        entry.value = resolve(&entry.value);
    }
    resolved.body = match &request.body {
        ManualBody::None => ManualBody::None,
        ManualBody::Form { fields } => ManualBody::Form {
            fields: fields
                .iter()
                .map(|(name, value)| (resolve(name), resolve(value)))
                .collect(),
        },
        ManualBody::Multipart { fields } => ManualBody::Multipart {
            fields: fields
                .iter()
                .map(|field| MultipartField {
                    name: resolve(&field.name),
                    value: field.value.as_deref().map(resolve),
                    file: field.file.as_deref().map(resolve),
                    media_type: field.media_type.clone(),
                })
                .collect(),
        },
        ManualBody::Raw { media_type, text } => ManualBody::Raw {
            media_type: media_type.clone(),
            text: resolve(text),
        },
        ManualBody::Binary { bytes } => ManualBody::Binary {
            bytes: bytes.clone(),
        },
    };
    resolved.auth = match &request.auth {
        AuthSpec::None => AuthSpec::None,
        AuthSpec::Bearer { token } => AuthSpec::Bearer {
            token: resolve(token),
        },
        AuthSpec::Basic { username, password } => AuthSpec::Basic {
            username: resolve(username),
            password: resolve(password),
        },
        AuthSpec::ApiKey {
            key,
            value,
            in_query,
        } => AuthSpec::ApiKey {
            key: resolve(key),
            value: resolve(value),
            in_query: *in_query,
        },
    };
    resolved
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::composer::model::MultipartField;

    fn variables() -> Vec<Variable> {
        vec![
            Variable {
                name: "host".into(),
                value: "api.prod.test".into(),
                is_secret: false,
            },
            Variable {
                name: "token".into(),
                value: "s3cret".into(),
                is_secret: true,
            },
        ]
    }

    #[test]
    fn resolves_placeholders_in_a_single_pass() {
        let variables = variables();
        assert_eq!(
            resolve("https://{{host}}/v1?k={{token}}", &variables),
            "https://api.prod.test/v1?k=s3cret"
        );
        // Values never chain-expand: {{host}} is not a value here.
        let nested = vec![Variable {
            name: "a".into(),
            value: "{{b}}".into(),
            is_secret: false,
        }];
        assert_eq!(resolve("{{a}}", &nested), "{{b}}");
    }

    #[test]
    fn leaves_unknown_and_malformed_placeholders_untouched() {
        let variables = variables();
        assert_eq!(
            resolve("{{missing}} and {{host}}", &variables),
            "{{missing}} and api.prod.test"
        );
        assert_eq!(resolve("unclosed {{host", &variables), "unclosed {{host");
        assert_eq!(resolve("{{}}", &variables), "{{}}");
        assert_eq!(resolve("no placeholders", &variables), "no placeholders");
    }

    #[test]
    fn resolves_every_request_field() {
        let request = ManualRequest {
            method: "POST".into(),
            url: "https://{{host}}/items".into(),
            query: vec![crate::traffic::QueryParameter {
                name: "{{host}}".into(),
                value: "v".into(),
            }],
            headers: vec![crate::traffic::HeaderEntry {
                name: "Authorization".into(),
                value: "Bearer {{token}}".into(),
            }],
            body: ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"t\":\"{{token}}\"}".into(),
            },
            auth: AuthSpec::Bearer {
                token: "{{token}}".into(),
            },
        };
        let resolved = resolve_request(&request, &variables());
        assert_eq!(resolved.url, "https://api.prod.test/items");
        assert_eq!(resolved.query[0].name, "api.prod.test");
        assert_eq!(resolved.headers[0].value, "Bearer s3cret");
        assert_eq!(
            resolved.body,
            ManualBody::Raw {
                media_type: Some("application/json".into()),
                text: "{\"t\":\"s3cret\"}".into()
            }
        );
        assert_eq!(
            resolved.auth,
            AuthSpec::Bearer {
                token: "s3cret".into()
            }
        );
        // The original request keeps its placeholders.
        assert_eq!(request.url, "https://{{host}}/items");
    }

    #[test]
    fn resolves_multipart_fields_and_files() {
        let request = ManualRequest {
            method: "POST".into(),
            url: "https://{{host}}/upload".into(),
            query: vec![],
            headers: vec![],
            body: ManualBody::Multipart {
                fields: vec![
                    MultipartField {
                        name: "file".into(),
                        value: None,
                        file: Some("/tmp/{{file_name}}".into()),
                        media_type: None,
                    },
                    MultipartField {
                        name: "note".into(),
                        value: Some("hi {{token}}".into()),
                        file: None,
                        media_type: None,
                    },
                ],
            },
            auth: AuthSpec::None,
        };
        let variables = vec![
            Variable {
                name: "file_name".into(),
                value: "a.txt".into(),
                is_secret: false,
            },
            Variable {
                name: "token".into(),
                value: "s3cret".into(),
                is_secret: true,
            },
        ];
        let resolved = resolve_request(&request, &variables);
        let ManualBody::Multipart { fields } = &resolved.body else {
            panic!("expected multipart body");
        };
        assert_eq!(fields[0].file.as_deref(), Some("/tmp/a.txt"));
        assert_eq!(fields[1].value.as_deref(), Some("hi s3cret"));
    }
}
